use rmcp::{ClientHandler, ServiceExt};

#[derive(Default, Clone)]
struct TestClient;
impl ClientHandler for TestClient {}

#[tokio::test]
async fn test_get_version_tool() -> anyhow::Result<()> {
    use goldentooth_mcp::tools::version::GoldentoothMcp;
    use rmcp::model::*;

    let server = GoldentoothMcp::new();
    let client = TestClient::default();

    let (server_transport, client_transport) = tokio::io::duplex(4096);
    let _server_handle = tokio::spawn(async move {
        let service = server.serve(server_transport).await.unwrap();
        service.waiting().await.unwrap();
    });

    let client_service = client.serve(client_transport).await?;

    // List tools and verify get_version is present.
    let tools_result = client_service
        .send_request(ClientRequest::ListToolsRequest(
            RequestOptionalParam::default(),
        ))
        .await?;

    let ServerResult::ListToolsResult(tools) = tools_result else {
        panic!("expected ListToolsResult");
    };
    assert!(tools.tools.iter().any(|t| t.name == "get_version"));

    // Call get_version and check the response.
    let call_result = client_service
        .send_request(ClientRequest::CallToolRequest(Request::new(
            CallToolRequestParams::new("get_version"),
        )))
        .await?;

    let ServerResult::CallToolResult(result) = call_result else {
        panic!("expected CallToolResult");
    };

    let text = result.content[0]
        .as_text()
        .expect("expected text content")
        .text
        .as_str();
    let parsed: serde_json::Value = serde_json::from_str(text)?;
    assert_eq!(parsed["server"], "goldentooth-mcp");
    assert_eq!(parsed["version"], env!("CARGO_PKG_VERSION"));

    client_service.cancel().await?;
    Ok(())
}
