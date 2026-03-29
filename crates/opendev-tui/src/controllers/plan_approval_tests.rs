use super::*;

#[test]
fn test_new_is_inactive() {
    let ctrl = PlanApprovalController::new();
    assert!(!ctrl.active());
    assert_eq!(ctrl.status(), PlanStatus::Pending);
}

#[tokio::test]
async fn test_start_activates() {
    let mut ctrl = PlanApprovalController::new();
    let _rx = ctrl.start("Step 1: Do X\nStep 2: Do Y".into());
    assert!(ctrl.active());
    assert_eq!(ctrl.options().len(), 3);
    assert_eq!(ctrl.selected_action(), 0);
    assert!(ctrl.plan_content().contains("Step 1"));
}

#[tokio::test]
async fn test_approve_sends_decision() {
    let mut ctrl = PlanApprovalController::new();
    let rx = ctrl.start("plan".into());
    let decision = ctrl.approve().unwrap();
    assert_eq!(decision.action, "approve_auto");
    assert!(!ctrl.active());

    let received = rx.await.unwrap();
    assert_eq!(received.action, "approve_auto");
}

#[tokio::test]
async fn test_reject_sends_modify() {
    let mut ctrl = PlanApprovalController::new();
    let rx = ctrl.start("plan".into());
    let decision = ctrl.reject().unwrap();
    assert_eq!(decision.action, "modify");

    let received = rx.await.unwrap();
    assert_eq!(received.action, "modify");
}

#[tokio::test]
async fn test_next_prev_wraps() {
    let mut ctrl = PlanApprovalController::new();
    let _rx = ctrl.start("plan".into());

    ctrl.next();
    assert_eq!(ctrl.selected_action(), 1);
    ctrl.next();
    assert_eq!(ctrl.selected_action(), 2);
    ctrl.next();
    assert_eq!(ctrl.selected_action(), 0); // wrap

    ctrl.prev();
    assert_eq!(ctrl.selected_action(), 2); // wrap back
}

#[tokio::test]
async fn test_cancel_selects_revise() {
    let mut ctrl = PlanApprovalController::new();
    let rx = ctrl.start("plan".into());
    ctrl.cancel();
    assert!(!ctrl.active());

    let received = rx.await.unwrap();
    assert_eq!(received.action, "modify");
}
