use quectoclaw::mcp::MCPClient;
use serde_json::json;
use std::collections::HashMap;
use std::sync::Arc;

#[tokio::test]
async fn test_mcp_client_mock() {
    let mock_script = "/home/pain/Desktop/QuectoClaw/tests/mock_mcp.py";
    let client = MCPClient::spawn(
        "mock",
        "python3",
        &[mock_script.to_string()],
        &HashMap::new(),
    )
    .await
    .unwrap();

    let client_arc = Arc::new(client);

    // 1. Initialize
    let init_params = json!({
        "protocolVersion": "2024-11-05",
        "capabilities": {},
        "clientInfo": { "name": "TestClient", "version": "1.0.0" }
    });

    println!("Calling initialize...");
    let resp = client_arc.call("initialize", init_params).await.unwrap();
    println!("Initialize response: {:?}", resp);
    assert_eq!(
        resp.get("protocolVersion").and_then(|v| v.as_str()),
        Some("2024-11-05")
    );

    // 2. List tools
    println!("Calling tools/list...");
    let tools = client_arc.call("tools/list", json!({})).await.unwrap();
    println!("Tools response: {:?}", tools);
    let tools_arr = tools.get("tools").and_then(|t| t.as_array()).unwrap();
    assert!(tools_arr
        .iter()
        .any(|t| t.get("name").and_then(|n| n.as_str()) == Some("echo")));

    // 3. Call tool
    println!("Calling tools/call (echo)...");
    let call_params = json!({
        "name": "echo",
        "arguments": { "text": "hello mcp" }
    });
    let result = client_arc.call("tools/call", call_params).await.unwrap();
    println!("Call response: {:?}", result);
    let content = result.get("content").and_then(|c| c.as_array()).unwrap();
    let text = content[0].get("text").and_then(|t| t.as_str()).unwrap();
    assert_eq!(text, "MOCK_ECHO: hello mcp");
}
