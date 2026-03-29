use super::*;

#[tokio::test]
async fn test_read_message_basic() {
    let input = b"Content-Length: 14\r\n\r\n{\"hello\":true}";
    let mut reader = tokio::io::BufReader::new(&input[..]);
    let body = read_message(&mut reader).await.unwrap();
    assert_eq!(body, b"{\"hello\":true}");
}

#[tokio::test]
async fn test_read_message_with_extra_header() {
    let input = b"Content-Length: 2\r\nContent-Type: application/json\r\n\r\n{}";
    let mut reader = tokio::io::BufReader::new(&input[..]);
    let body = read_message(&mut reader).await.unwrap();
    assert_eq!(body, b"{}");
}

#[tokio::test]
async fn test_read_message_missing_content_length() {
    let input = b"X-Custom: foo\r\n\r\n{}";
    let mut reader = tokio::io::BufReader::new(&input[..]);
    let result = read_message(&mut reader).await;
    assert!(result.is_err());
}
