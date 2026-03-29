use super::*;

#[tokio::test]
async fn test_plan_approval_roundtrip() {
    let (tx, mut rx) = plan_approval_channel();
    let (resp_tx, resp_rx) = oneshot::channel();

    tx.send(PlanApprovalRequest {
        plan_content: "Step 1: Do X".into(),
        response_tx: resp_tx,
    })
    .unwrap();

    let req = rx.recv().await.unwrap();
    assert!(req.plan_content.contains("Step 1"));

    req.response_tx
        .send(PlanDecision {
            action: "approve_auto".into(),
            feedback: String::new(),
        })
        .unwrap();

    let decision = resp_rx.await.unwrap();
    assert_eq!(decision.action, "approve_auto");
}
