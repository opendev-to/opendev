use super::*;

#[test]
fn test_stdio_transport_not_connected() {
    let transport = StdioTransport::new(
        "node".to_string(),
        vec!["server.js".to_string()],
        HashMap::new(),
    );
    assert!(!transport.is_connected());
    assert_eq!(transport.command(), "node");
    assert_eq!(transport.args(), &["server.js"]);
}

#[tokio::test]
async fn test_stdio_connect_and_echo() {
    // Spawn a simple cat-like echo process that reads Content-Length
    // framed messages and writes them back. We use a small Python script.
    let script = r#"
import sys
while True:
    line = sys.stdin.readline()
    if not line:
        break
    if line.startswith("Content-Length:"):
        length = int(line.split(":")[1].strip())
        sys.stdin.readline()  # empty line
        body = sys.stdin.read(length)
        response = body
        header = f"Content-Length: {len(response)}\r\n\r\n"
        sys.stdout.write(header)
        sys.stdout.write(response)
        sys.stdout.flush()
"#;

    let mut transport = StdioTransport::new(
        "python3".to_string(),
        vec!["-u".to_string(), "-c".to_string(), script.to_string()],
        HashMap::new(),
    );

    transport.connect().await.unwrap();
    assert!(transport.is_connected());

    let request = JsonRpcRequest {
        jsonrpc: "2.0".to_string(),
        id: 1,
        method: "test".to_string(),
        params: None,
    };

    let response = transport.send_request(&request).await.unwrap();
    assert_eq!(response.jsonrpc, "2.0");

    transport.close().await.unwrap();
}
