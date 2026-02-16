use quectoclaw::agent::AgentLoop;
use quectoclaw::config::Config;
use quectoclaw::provider::http::HTTPProvider;
use quectoclaw::tool::exec::ExecTool;
use quectoclaw::tool::ToolRegistry;
use quectoclaw::bus::MessageBus;
use std::sync::Arc;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};
use serde_json::json;
use std::collections::HashMap;

#[tokio::test]
async fn test_agent_integration_with_tool() {
    // 1. Start mock server
    let mock_server = MockServer::start().await;

    // 2. Prepare mock LLM response (Tool Call)
    let tool_call_resp = json!({
        "id": "chatcmpl-123",
        "object": "chat.completion",
        "created": 1677652288,
        "model": "gpt-4o",
        "choices": [{
            "index": 0,
            "message": {
                "role": "assistant",
                "content": null,
                "tool_calls": [{
                    "id": "call_abc123",
                    "type": "function",
                    "function": {
                        "name": "exec",
                        "arguments": "{\"command\":\"echo hello-test\"}"
                    }
                }]
            },
            "finish_reason": "tool_calls"
        }],
        "usage": {
            "prompt_tokens": 9,
            "completion_tokens": 12,
            "total_tokens": 21
        }
    });

    // 3. Prepare mock LLM response (Final Answer)
    let final_resp = json!({
        "id": "chatcmpl-456",
        "object": "chat.completion",
        "created": 1677652289,
        "model": "gpt-4o",
        "choices": [{
            "index": 0,
            "message": {
                "role": "assistant",
                "content": "The command returned hello-test"
            },
            "finish_reason": "stop"
        }],
        "usage": {
            "prompt_tokens": 40,
            "completion_tokens": 10,
            "total_tokens": 50
        }
    });

    // Mock second call (final answer)
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .and(wiremock::matchers::body_string_contains("\"role\":\"tool\""))
        .respond_with(ResponseTemplate::new(200).set_body_json(final_resp))
        .expect(1)
        .mount(&mock_server)
        .await;

    // Mock first call (tool call)
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .and(wiremock::matchers::body_string_contains("\"role\":\"user\""))
        // Important: ensure it doesn't match once tool results are present
        .and(wiremock::matchers::body_partial_json(json!({
            "messages": [
                { "role": "system" },
                { "role": "user" }
            ]
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(tool_call_resp))
        .expect(1)
        .mount(&mock_server)
        .await;

    // 4. Setup Agent
    let mut config = Config::default();
    config.providers.openai.api_base = mock_server.uri();
    config.providers.openai.api_key = "test-key".to_string();
    
    // Create a temporary workspace
    let tmp_dir = tempfile::tempdir().unwrap();
    let ws_path = tmp_dir.path().to_string_lossy().to_string();
    config.agents.defaults.workspace = ws_path.clone();

    let provider = Arc::new(HTTPProvider::new(
        config.providers.openai.api_key.clone(),
        config.providers.openai.api_base.clone(),
        None,
        config.agents.defaults.model.clone(),
    ).unwrap());

    let registry = ToolRegistry::new();
    registry.register(Arc::new(ExecTool::new(ws_path, false))).await;

    let bus = Arc::new(MessageBus::new());
    let agent = AgentLoop::new(config, provider, registry, bus);

    // 5. Run agent loop
    let result = agent.run_agent_loop("say hello", "test-session", true, None::<tokio::sync::mpsc::Sender<quectoclaw::provider::StreamEvent>>).await.unwrap();

    println!("AGENT RESULT: {}", result);

    // 6. Assertions
    assert!(result.contains("hello-test"));
}
