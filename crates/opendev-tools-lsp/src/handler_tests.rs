use super::*;

#[test]
fn test_handler_not_ready_initially() {
    let config = ServerConfig {
        command: "test-server".to_string(),
        args: vec![],
        language_id: "test".to_string(),
        extensions: vec!["test".to_string()],
    };
    let handler = LspHandler::new(config, PathBuf::from("/tmp"));
    assert!(!handler.is_ready());
}

#[tokio::test]
async fn test_send_request_without_start_fails() {
    let config = ServerConfig {
        command: "test-server".to_string(),
        args: vec![],
        language_id: "test".to_string(),
        extensions: vec!["test".to_string()],
    };
    let handler = LspHandler::new(config, PathBuf::from("/tmp"));
    let result = handler.send_request("test/method", Value::Null).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_read_message_eof() {
    let data: &[u8] = b"";
    let mut reader = BufReader::new(data);
    let result = read_message(&mut reader).await.unwrap();
    assert!(result.is_none());
}

#[tokio::test]
async fn test_read_message_valid() {
    let body = r#"{"jsonrpc":"2.0","id":1,"result":null}"#;
    let msg = format!("Content-Length: {}\r\n\r\n{}", body.len(), body);
    let mut reader = BufReader::new(msg.as_bytes());
    let result = read_message(&mut reader).await.unwrap();
    assert!(result.is_some());
    let val = result.unwrap();
    assert_eq!(val["id"], 1);
}

#[tokio::test]
async fn test_read_message_missing_content_length() {
    let msg = b"\r\n{\"test\":true}";
    let mut reader = BufReader::new(msg.as_slice());
    let result = read_message(&mut reader).await;
    assert!(result.is_err());
}
