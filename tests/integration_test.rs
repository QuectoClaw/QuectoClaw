use quectoclaw::agent::AgentLoop;
use quectoclaw::bus::MessageBus;
use quectoclaw::config::Config;
use quectoclaw::provider::http::HTTPProvider;
use quectoclaw::tool::exec::ExecTool;
use quectoclaw::tool::ToolRegistry;
use serde_json::json;
use std::sync::Arc;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn init_tracing() {
    let _ = tracing_subscriber::fmt::try_init();
}

#[tokio::test]
async fn test_agent_integration_with_tool() {
    init_tracing();
    let mock_server = MockServer::start().await;

    // Turn 1: User Message -> Exec Tool Call - mount first
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .and(wiremock::matchers::body_string_contains(
            "\"role\":\"user\"",
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "object": "chat.completion",
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
            }]
        })))
        .up_to_n_times(1)
        .expect(1)
        .mount(&mock_server)
        .await;

    // Turn 2: Tool Result -> Final Answer - mount last
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .and(wiremock::matchers::body_string_contains(
            "\"role\":\"tool\"",
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "object": "chat.completion",
            "choices": [{
                "index": 0,
                "message": {
                    "role": "assistant",
                    "content": "The command returned hello-test"
                },
                "finish_reason": "stop"
            }]
        })))
        .up_to_n_times(1)
        .expect(1)
        .mount(&mock_server)
        .await;

    let mut config = Config::default();
    config.providers.openai.api_base = mock_server.uri();
    config.providers.openai.api_key = "test-key".to_string();

    let tmp_dir = tempfile::tempdir().unwrap();
    let ws_path = tmp_dir.path().to_string_lossy().to_string();
    config.agents.defaults.workspace = ws_path.clone();

    let provider = Arc::new(
        HTTPProvider::new(
            config.providers.openai.api_key.clone(),
            config.providers.openai.api_base.clone(),
            None,
            config.agents.defaults.model.clone(),
        )
        .unwrap(),
    );

    let registry = ToolRegistry::new();
    registry
        .register(Arc::new(ExecTool::new(ws_path, false, vec![], vec![])))
        .await;

    let bus = Arc::new(MessageBus::new());
    let agent = AgentLoop::new(config, provider, registry, bus);

    let result = agent
        .run_agent_loop("say hello", "test-session", true, None)
        .await
        .unwrap();

    assert!(result.contains("hello-test"));
}

#[tokio::test]
async fn test_agent_retry_on_500() {
    init_tracing();
    let mock_server = MockServer::start().await;

    // Error once, then success

    // Success Mock (General) - Priority 1
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "object": "chat.completion",
            "choices": [{
                "index": 0,
                "message": { "role": "assistant", "content": "Retry worked" },
                "finish_reason": "stop"
            }]
        })))
        .mount(&mock_server)
        .await;

    // Error Mock (Highest priority) - Mounted last
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(ResponseTemplate::new(500))
        .up_to_n_times(1)
        .mount(&mock_server)
        .await;

    let mut config = Config::default();
    config.agents.defaults.max_retries = 2;
    config.agents.defaults.retry_delay_ms = 10;

    let provider = Arc::new(
        HTTPProvider::new("test-key".into(), mock_server.uri(), None, "gpt-4o".into()).unwrap(),
    );

    let registry = ToolRegistry::new();
    let bus = Arc::new(MessageBus::new());
    let agent = AgentLoop::new(config, provider, registry, bus);

    let result = agent
        .run_agent_loop("test retry", "test-session", false, None)
        .await
        .unwrap();
    assert_eq!(result, "Retry worked");
}

#[tokio::test]
async fn test_tool_failure_handling() {
    init_tracing();
    let mock_server = MockServer::start().await;

    // Turn 1: LLM calls read_file (non-existent) - mount first
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .and(wiremock::matchers::body_string_contains(
            "\"role\":\"user\"",
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "object": "chat.completion",
            "choices": [{
                "index": 0,
                "message": {
                    "role": "assistant",
                    "content": null,
                    "tool_calls": [{
                        "id": "call_err_123",
                        "type": "function",
                        "function": {
                            "name": "read_file",
                            "arguments": "{\"path\":\"non_existent.txt\"}"
                        }
                    }]
                },
                "finish_reason": "tool_calls"
            }]
        })))
        .up_to_n_times(1)
        .expect(1)
        .mount(&mock_server)
        .await;

    // Turn 2: Final Answer based on error - mount last
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .and(wiremock::matchers::body_string_contains(
            "\"role\":\"tool\"",
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "object": "chat.completion",
            "choices": [{
                "index": 0,
                "message": {
                    "role": "assistant",
                    "content": "The file does not exist."
                },
                "finish_reason": "stop"
            }]
        })))
        .up_to_n_times(1)
        .expect(1)
        .mount(&mock_server)
        .await;

    let mut config = Config::default();
    let tmp_dir = tempfile::tempdir().unwrap();
    let ws_path = tmp_dir.path().to_string_lossy().to_string();
    config.agents.defaults.workspace = ws_path.clone();

    let provider = Arc::new(
        HTTPProvider::new("test-key".into(), mock_server.uri(), None, "gpt-4o".into()).unwrap(),
    );

    let registry = ToolRegistry::new();
    use quectoclaw::tool::filesystem::ReadFileTool;
    registry
        .register(Arc::new(ReadFileTool::new(ws_path, true)))
        .await;

    let bus = Arc::new(MessageBus::new());
    let agent = AgentLoop::new(config, provider, registry, bus);

    let result = agent
        .run_agent_loop("read missing file", "test-session", false, None)
        .await
        .unwrap();

    assert!(result.contains("does not exist"));
}
