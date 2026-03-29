use super::*;

#[tokio::test]
async fn test_tool_approval_roundtrip() {
    let (tx, mut rx) = tool_approval_channel();
    let (resp_tx, resp_rx) = oneshot::channel();

    tx.send(ToolApprovalRequest {
        tool_name: "bash".into(),
        command: "rm -rf /tmp/test".into(),
        working_dir: "/home/user".into(),
        response_tx: resp_tx,
    })
    .unwrap();

    let req = rx.recv().await.unwrap();
    assert_eq!(req.tool_name, "bash");
    assert!(req.command.contains("rm"));

    req.response_tx
        .send(ToolApprovalDecision {
            approved: true,
            choice: "yes".into(),
            command: req.command,
        })
        .unwrap();

    let decision = resp_rx.await.unwrap();
    assert!(decision.approved);
    assert_eq!(decision.choice, "yes");
}
