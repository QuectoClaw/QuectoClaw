// QuectoClaw — HTTP-based LLM provider (OpenAI-compatible)

use super::*;
use async_trait::async_trait;
use reqwest::Client;
use serde_json::json;
use std::collections::HashMap;
use std::time::Duration;

/// HTTPProvider speaks the OpenAI-compatible chat completions API.
/// Works with OpenAI, OpenRouter, Groq, Zhipu, vLLM, Moonshot, etc.
pub struct HTTPProvider {
    api_key: String,
    api_base: String,
    client: Client,
    model: String,
}

impl HTTPProvider {
    pub fn new(api_key: String, api_base: String, proxy: Option<&str>, model: String) -> anyhow::Result<Self> {
        let mut builder = Client::builder()
            .timeout(Duration::from_secs(120))
            .connect_timeout(Duration::from_secs(30));

        if let Some(proxy_url) = proxy {
            if !proxy_url.is_empty() {
                builder = builder.proxy(reqwest::Proxy::all(proxy_url)?);
            }
        }

        let base = if api_base.is_empty() {
            // Default API bases by provider detection
            if api_key.starts_with("sk-or-") {
                "https://openrouter.ai/api/v1".to_string()
            } else if api_key.starts_with("gsk_") {
                "https://api.groq.com/openai/v1".to_string()
            } else if api_key.starts_with("nvapi-") {
                "https://integrate.api.nvidia.com/v1".to_string()
            } else {
                "https://api.openai.com/v1".to_string()
            }
        } else {
            api_base
        };

        Ok(Self {
            api_key,
            api_base: base,
            client: builder.build()?,
            model,
        })
    }
}

#[async_trait]
impl LLMProvider for HTTPProvider {
    async fn chat(
        &self,
        messages: &[Message],
        tools: &[ToolDefinition],
        model: &str,
        options: &HashMap<String, serde_json::Value>,
    ) -> anyhow::Result<LLMResponse> {
        let use_model = if model.is_empty() { &self.model } else { model };
        let url = format!("{}/chat/completions", self.api_base.trim_end_matches('/'));

        // Build request body
        let mut body = json!({
            "model": use_model,
            "messages": messages,
        });

        // Add tools if any
        if !tools.is_empty() {
            body["tools"] = serde_json::to_value(tools)?;
        }

        // Apply options
        if let Some(obj) = body.as_object_mut() {
            for (k, v) in options {
                obj.insert(k.clone(), v.clone());
            }
        }

        tracing::debug!(
            url = %url,
            model = %use_model,
            messages = messages.len(),
            tools = tools.len(),
            "Sending LLM request"
        );

        let response = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await?;

        let status = response.status();
        let response_body = response.text().await?;

        if !status.is_success() {
            anyhow::bail!("LLM API error ({}): {}", status, response_body);
        }

        tracing::debug!(status = %status, body_len = response_body.len(), "LLM response received");

        parse_response(&response_body)
    }

    fn default_model(&self) -> &str {
        &self.model
    }
}

/// Parse an OpenAI-compatible chat completion response.
fn parse_response(body: &str) -> anyhow::Result<LLMResponse> {
    let v: serde_json::Value = serde_json::from_str(body)?;

    // Check for API error
    if let Some(err) = v.get("error") {
        let msg = err.get("message").and_then(|m| m.as_str()).unwrap_or("unknown error");
        anyhow::bail!("LLM API error: {}", msg);
    }

    let choices = v.get("choices").and_then(|c| c.as_array());
    let choice = choices
        .and_then(|c| c.first())
        .ok_or_else(|| anyhow::anyhow!("No choices in LLM response"))?;

    let message = choice.get("message")
        .ok_or_else(|| anyhow::anyhow!("No message in choice"))?;

    let content = message.get("content")
        .and_then(|c| c.as_str())
        .unwrap_or("")
        .to_string();

    let finish_reason = choice.get("finish_reason")
        .and_then(|f| f.as_str())
        .unwrap_or("stop")
        .to_string();

    // Parse tool calls
    let tool_calls = if let Some(tc_array) = message.get("tool_calls").and_then(|t| t.as_array()) {
        let mut calls = Vec::new();
        for tc in tc_array {
            let id = tc.get("id").and_then(|i| i.as_str()).unwrap_or("").to_string();
            let call_type = tc.get("type").and_then(|t| t.as_str()).map(|s| s.to_string());

            let function = if let Some(func) = tc.get("function") {
                let name = func.get("name").and_then(|n| n.as_str()).unwrap_or("").to_string();
                let arguments = func.get("arguments").and_then(|a| a.as_str()).unwrap_or("{}").to_string();
                Some(FunctionCall { name, arguments })
            } else {
                None
            };

            calls.push(ToolCall {
                id,
                call_type,
                function,
                name: None,
                arguments: None,
            });
        }
        if calls.is_empty() { None } else { Some(calls) }
    } else {
        None
    };

    // Parse usage
    let usage = v.get("usage").map(|u| UsageInfo {
        prompt_tokens: u.get("prompt_tokens").and_then(|n| n.as_u64()).unwrap_or(0) as usize,
        completion_tokens: u.get("completion_tokens").and_then(|n| n.as_u64()).unwrap_or(0) as usize,
        total_tokens: u.get("total_tokens").and_then(|n| n.as_u64()).unwrap_or(0) as usize,
    });

    Ok(LLMResponse {
        content,
        tool_calls,
        finish_reason,
        usage,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_response() {
        let json = r#"{
            "choices": [{
                "message": {"role": "assistant", "content": "Hello!"},
                "finish_reason": "stop"
            }],
            "usage": {"prompt_tokens": 10, "completion_tokens": 5, "total_tokens": 15}
        }"#;

        let resp = parse_response(json).unwrap();
        assert_eq!(resp.content, "Hello!");
        assert_eq!(resp.finish_reason, "stop");
        assert!(!resp.has_tool_calls());
        assert_eq!(resp.usage.unwrap().total_tokens, 15);
    }

    #[test]
    fn test_parse_tool_call_response() {
        let json = r#"{
            "choices": [{
                "message": {
                    "role": "assistant",
                    "content": "",
                    "tool_calls": [{
                        "id": "call_1",
                        "type": "function",
                        "function": {
                            "name": "read_file",
                            "arguments": "{\"path\": \"test.txt\"}"
                        }
                    }]
                },
                "finish_reason": "tool_calls"
            }]
        }"#;

        let resp = parse_response(json).unwrap();
        assert!(resp.has_tool_calls());
        let tc = &resp.tool_calls.unwrap()[0];
        assert_eq!(tc.function_name(), "read_file");
        let args = tc.parsed_arguments();
        assert_eq!(args.get("path").unwrap().as_str().unwrap(), "test.txt");
    }

    #[test]
    fn test_parse_error_response() {
        let json = r#"{"error": {"message": "Invalid API key", "type": "auth_error"}}"#;
        let result = parse_response(json);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Invalid API key"));
    }
}
