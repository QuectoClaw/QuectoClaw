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
    pub fn new(
        api_key: String,
        api_base: String,
        proxy: Option<&str>,
        model: String,
    ) -> anyhow::Result<Self> {
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

        let max_retries = options
            .get("max_retries")
            .and_then(|v| v.as_u64())
            .unwrap_or(3) as usize;
        let retry_delay_ms = options
            .get("retry_delay_ms")
            .and_then(|v| v.as_u64())
            .unwrap_or(1000);

        // Build request body once
        let mut body = json!({
            "model": use_model,
            "messages": messages,
        });

        if !tools.is_empty() {
            body["tools"] = serde_json::to_value(tools)?;
        }

        if let Some(obj) = body.as_object_mut() {
            for (k, v) in options {
                // Don't leak retry config into the API call itself
                if k != "max_retries" && k != "retry_delay_ms" {
                    obj.insert(k.clone(), v.clone());
                }
            }
        }

        let mut last_error = None;
        for attempt in 0..=max_retries {
            if attempt > 0 {
                tracing::info!(
                    attempt = attempt,
                    "Retrying LLM request after {}ms delay",
                    retry_delay_ms
                );
                tokio::time::sleep(tokio::time::Duration::from_millis(retry_delay_ms)).await;
            }

            tracing::debug!(
                url = %url,
                model = %use_model,
                attempt = attempt,
                "Sending LLM request"
            );

            let res = self
                .client
                .post(&url)
                .header("Authorization", format!("Bearer {}", self.api_key))
                .header("Content-Type", "application/json")
                .json(&body)
                .send()
                .await;

            match res {
                Ok(response) => {
                    let status = response.status();
                    if status.is_success() {
                        let response_body = response.text().await?;
                        tracing::debug!(status = %status, body_len = response_body.len(), "LLM response received");
                        return parse_response(&response_body);
                    }

                    let is_transient = status.is_server_error() || status.as_u16() == 429;
                    let response_body = response
                        .text()
                        .await
                        .unwrap_or_else(|_| "could not read body".to_string());

                    if is_transient && attempt < max_retries {
                        tracing::warn!(status = %status, attempt = attempt, "Transient LLM API error: {}", response_body);
                        last_error = Some(anyhow::anyhow!(
                            "LLM API error ({}): {}",
                            status,
                            response_body
                        ));
                        continue;
                    } else {
                        anyhow::bail!("LLM API error ({}): {}", status, response_body);
                    }
                }
                Err(e) if attempt < max_retries => {
                    tracing::warn!(error = %e, attempt = attempt, "Network error during LLM request");
                    last_error = Some(anyhow::Error::from(e));
                    continue;
                }
                Err(e) => return Err(anyhow::Error::from(e)),
            }
        }

        Err(last_error.unwrap_or_else(|| {
            anyhow::anyhow!("LLM request failed after {} attempts", max_retries + 1)
        }))
    }

    async fn chat_stream(
        &self,
        messages: &[Message],
        tools: &[ToolDefinition],
        model: &str,
        options: &HashMap<String, serde_json::Value>,
        tx: tokio::sync::mpsc::Sender<StreamEvent>,
    ) -> anyhow::Result<()> {
        let use_model = if model.is_empty() { &self.model } else { model };
        let url = format!("{}/chat/completions", self.api_base.trim_end_matches('/'));

        let max_retries = options
            .get("max_retries")
            .and_then(|v| v.as_u64())
            .unwrap_or(3) as usize;
        let retry_delay_ms = options
            .get("retry_delay_ms")
            .and_then(|v| v.as_u64())
            .unwrap_or(1000);

        let mut body = json!({
            "model": use_model,
            "messages": messages,
            "stream": true,
        });

        if !tools.is_empty() {
            body["tools"] = serde_json::to_value(tools)?;
        }

        if let Some(obj) = body.as_object_mut() {
            for (k, v) in options {
                if k != "max_retries" && k != "retry_delay_ms" {
                    obj.insert(k.clone(), v.clone());
                }
            }
        }

        let mut response = None;
        for attempt in 0..=max_retries {
            if attempt > 0 {
                tracing::info!(
                    attempt = attempt,
                    "Retrying LLM stream request after {}ms delay",
                    retry_delay_ms
                );
                tokio::time::sleep(tokio::time::Duration::from_millis(retry_delay_ms)).await;
            }

            let res = self
                .client
                .post(&url)
                .header("Authorization", format!("Bearer {}", self.api_key))
                .header("Content-Type", "application/json")
                .json(&body)
                .send()
                .await;

            match res {
                Ok(resp) => {
                    let status = resp.status();
                    if status.is_success() {
                        response = Some(resp);
                        break;
                    }

                    let is_transient = status.is_server_error() || status.as_u16() == 429;
                    let err_body = resp.text().await.unwrap_or_default();

                    if is_transient && attempt < max_retries {
                        tracing::warn!(status = %status, attempt = attempt, "Transient LLM stream error: {}", err_body);
                        continue;
                    } else {
                        let _ = tx
                            .send(StreamEvent::Error(format!("HTTP {}: {}", status, err_body)))
                            .await;
                        anyhow::bail!("LLM API error ({}): {}", status, err_body);
                    }
                }
                Err(e) if attempt < max_retries => {
                    tracing::warn!(error = %e, attempt = attempt, "Network error during LLM stream request");
                    continue;
                }
                Err(e) => {
                    let _ = tx.send(StreamEvent::Error(e.to_string())).await;
                    return Err(anyhow::Error::from(e));
                }
            }
        }

        let response = response.ok_or_else(|| {
            anyhow::anyhow!(
                "Failed to connect to LLM stream after {} attempts",
                max_retries + 1
            )
        })?;

        // Process SSE stream
        process_sse_stream(response, tx).await
    }

    fn default_model(&self) -> &str {
        &self.model
    }
}

/// Process an SSE byte stream into StreamEvents.
async fn process_sse_stream(
    response: reqwest::Response,
    tx: tokio::sync::mpsc::Sender<StreamEvent>,
) -> anyhow::Result<()> {
    use futures_util::StreamExt;

    let mut stream = response.bytes_stream();
    let mut buffer = String::new();

    // Accumulated state for building the final response
    let mut full_content = String::new();
    let mut finish_reason = String::from("stop");

    // Tool call accumulation: index -> (id, name, arguments)
    let mut tool_calls_acc: HashMap<usize, (String, String, String)> = HashMap::new();
    let mut usage_info: Option<UsageInfo> = None;

    while let Some(chunk_result) = stream.next().await {
        let chunk = match chunk_result {
            Ok(c) => c,
            Err(e) => {
                let _ = tx
                    .send(StreamEvent::Error(format!("Stream error: {}", e)))
                    .await;
                break;
            }
        };

        buffer.push_str(&String::from_utf8_lossy(&chunk));

        // Process complete lines
        while let Some(newline_pos) = buffer.find('\n') {
            let line = buffer[..newline_pos].trim().to_string();
            buffer = buffer[newline_pos + 1..].to_string();

            if line.is_empty() || line.starts_with(':') {
                continue;
            }

            if let Some(data) = line.strip_prefix("data: ") {
                let data = data.trim();
                if data == "[DONE]" {
                    // Build final response
                    let tool_calls = if tool_calls_acc.is_empty() {
                        None
                    } else {
                        let mut calls: Vec<_> = tool_calls_acc.drain().collect();
                        calls.sort_by_key(|(idx, _)| *idx);
                        Some(
                            calls
                                .into_iter()
                                .map(|(_, (id, name, args))| super::ToolCall {
                                    id,
                                    call_type: Some("function".into()),
                                    function: Some(super::FunctionCall {
                                        name,
                                        arguments: args,
                                    }),
                                    name: None,
                                    arguments: None,
                                })
                                .collect(),
                        )
                    };

                    let resp = LLMResponse {
                        content: full_content.clone(),
                        tool_calls,
                        finish_reason: finish_reason.clone(),
                        usage: usage_info.clone(),
                    };
                    let _ = tx.send(StreamEvent::Done(resp)).await;
                    return Ok(());
                }

                // Parse the JSON delta
                if let Ok(v) = serde_json::from_str::<serde_json::Value>(data) {
                    // Extract finish_reason
                    if let Some(fr) = v
                        .get("choices")
                        .and_then(|c| c.get(0))
                        .and_then(|c| c.get("finish_reason"))
                        .and_then(|f| f.as_str())
                    {
                        finish_reason = fr.to_string();
                    }

                    // Extract usage if present
                    if let Some(u) = v.get("usage") {
                        usage_info = Some(UsageInfo {
                            prompt_tokens: u
                                .get("prompt_tokens")
                                .and_then(|n| n.as_u64())
                                .unwrap_or(0) as usize,
                            completion_tokens: u
                                .get("completion_tokens")
                                .and_then(|n| n.as_u64())
                                .unwrap_or(0)
                                as usize,
                            total_tokens: u
                                .get("total_tokens")
                                .and_then(|n| n.as_u64())
                                .unwrap_or(0) as usize,
                        });
                    }

                    if let Some(delta) = v
                        .get("choices")
                        .and_then(|c| c.get(0))
                        .and_then(|c| c.get("delta"))
                    {
                        // Content delta
                        if let Some(content) = delta.get("content").and_then(|c| c.as_str()) {
                            if !content.is_empty() {
                                full_content.push_str(content);
                                let _ = tx.send(StreamEvent::Token(content.to_string())).await;
                            }
                        }

                        // Tool call deltas
                        if let Some(tcs) = delta.get("tool_calls").and_then(|t| t.as_array()) {
                            for tc in tcs {
                                let index =
                                    tc.get("index").and_then(|i| i.as_u64()).unwrap_or(0) as usize;

                                let id = tc.get("id").and_then(|i| i.as_str()).map(String::from);
                                let name = tc
                                    .get("function")
                                    .and_then(|f| f.get("name"))
                                    .and_then(|n| n.as_str())
                                    .map(String::from);
                                let args_fragment = tc
                                    .get("function")
                                    .and_then(|f| f.get("arguments"))
                                    .and_then(|a| a.as_str())
                                    .unwrap_or("");

                                let entry = tool_calls_acc.entry(index).or_insert_with(|| {
                                    (String::new(), String::new(), String::new())
                                });

                                if let Some(ref id_val) = id {
                                    entry.0 = id_val.clone();
                                }
                                if let Some(ref name_val) = name {
                                    entry.1 = name_val.clone();
                                }
                                entry.2.push_str(args_fragment);

                                let _ = tx
                                    .send(StreamEvent::ToolCallDelta {
                                        index,
                                        id,
                                        name,
                                        arguments: args_fragment.to_string(),
                                    })
                                    .await;
                            }
                        }
                    }
                }
            }
        }
    }

    // If stream ended without [DONE], send what we have
    if !full_content.is_empty() || !tool_calls_acc.is_empty() {
        let tool_calls = if tool_calls_acc.is_empty() {
            None
        } else {
            let mut calls: Vec<_> = tool_calls_acc.drain().collect();
            calls.sort_by_key(|(idx, _)| *idx);
            Some(
                calls
                    .into_iter()
                    .map(|(_, (id, name, args))| super::ToolCall {
                        id,
                        call_type: Some("function".into()),
                        function: Some(super::FunctionCall {
                            name,
                            arguments: args,
                        }),
                        name: None,
                        arguments: None,
                    })
                    .collect(),
            )
        };

        let resp = LLMResponse {
            content: full_content,
            tool_calls,
            finish_reason,
            usage: usage_info,
        };
        let _ = tx.send(StreamEvent::Done(resp)).await;
    }

    Ok(())
}

/// Parse an OpenAI-compatible chat completion response.
fn parse_response(body: &str) -> anyhow::Result<LLMResponse> {
    let v: serde_json::Value = serde_json::from_str(body)?;

    // Check for API error
    if let Some(err) = v.get("error") {
        let msg = err
            .get("message")
            .and_then(|m| m.as_str())
            .unwrap_or("unknown error");
        anyhow::bail!("LLM API error: {}", msg);
    }

    let choices = v.get("choices").and_then(|c| c.as_array());
    let choice = choices
        .and_then(|c| c.first())
        .ok_or_else(|| anyhow::anyhow!("No choices in LLM response"))?;

    let message = choice
        .get("message")
        .ok_or_else(|| anyhow::anyhow!("No message in choice"))?;

    let content = message
        .get("content")
        .and_then(|c| c.as_str())
        .unwrap_or("")
        .to_string();

    let finish_reason = choice
        .get("finish_reason")
        .and_then(|f| f.as_str())
        .unwrap_or("stop")
        .to_string();

    // Parse tool calls
    let tool_calls = if let Some(tc_array) = message.get("tool_calls").and_then(|t| t.as_array()) {
        let mut calls = Vec::new();
        for tc in tc_array {
            let id = tc
                .get("id")
                .and_then(|i| i.as_str())
                .unwrap_or("")
                .to_string();
            let call_type = tc
                .get("type")
                .and_then(|t| t.as_str())
                .map(|s| s.to_string());

            let function = if let Some(func) = tc.get("function") {
                let name = func
                    .get("name")
                    .and_then(|n| n.as_str())
                    .unwrap_or("")
                    .to_string();
                let arguments = func
                    .get("arguments")
                    .and_then(|a| a.as_str())
                    .unwrap_or("{}")
                    .to_string();
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
        if calls.is_empty() {
            None
        } else {
            Some(calls)
        }
    } else {
        None
    };

    // Parse usage
    let usage = v.get("usage").map(|u| UsageInfo {
        prompt_tokens: u.get("prompt_tokens").and_then(|n| n.as_u64()).unwrap_or(0) as usize,
        completion_tokens: u
            .get("completion_tokens")
            .and_then(|n| n.as_u64())
            .unwrap_or(0) as usize,
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
