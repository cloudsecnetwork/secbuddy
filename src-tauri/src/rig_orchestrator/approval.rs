//! Tiny helpers for the approval-gate state machine. The on-the-wire shape is
//! a `oneshot::Sender<String>` keyed by `invocation_id` inside a shared
//! `HashMap`, fired by `record_approval_and_execute` in `lib.rs`.
//!
//! These helpers exist so the state machine (pending → approved/denied/dry_run,
//! channel-closed → error) can be unit tested without spinning up a Tauri app.

#![allow(dead_code)] // exported for tests + future autonomous flows

use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use tokio::sync::oneshot;

pub type PendingApprovals = Arc<RwLock<HashMap<String, oneshot::Sender<String>>>>;

/// Possible outcomes after the user (or test driver) responds to an approval.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ApprovalOutcome {
    Approved,
    Denied,
    DryRun,
    Other(String),
}

impl ApprovalOutcome {
    pub fn from_decision(decision: &str) -> Self {
        match decision {
            "approved" => ApprovalOutcome::Approved,
            "denied" => ApprovalOutcome::Denied,
            "dry_run" => ApprovalOutcome::DryRun,
            other => ApprovalOutcome::Other(other.to_string()),
        }
    }
}

/// Register a pending approval and return its receiver. The orchestrator emits
/// `ApprovalRequired` after this so the UI can correlate by `invocation_id`.
pub fn register_pending(
    pending: &PendingApprovals,
    invocation_id: &str,
) -> oneshot::Receiver<String> {
    let (tx, rx) = oneshot::channel();
    pending
        .write()
        .unwrap()
        .insert(invocation_id.to_string(), tx);
    rx
}

/// Wait for the user's decision on a pending approval. The error string
/// ("Approval channel closed") is the contract `cancel_tool_invocation`
/// relies on when it clears `pending_approvals` mid-flight.
pub async fn await_decision(rx: oneshot::Receiver<String>) -> Result<ApprovalOutcome, String> {
    match rx.await {
        Ok(d) => Ok(ApprovalOutcome::from_decision(&d)),
        Err(_) => Err("Approval channel closed".to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn new_pending() -> PendingApprovals {
        Arc::new(RwLock::new(HashMap::new()))
    }

    #[tokio::test]
    async fn approved_decision_round_trips() {
        let pending = new_pending();
        let rx = register_pending(&pending, "inv-1");
        let tx = pending.write().unwrap().remove("inv-1").unwrap();
        tx.send("approved".to_string()).unwrap();
        assert_eq!(
            await_decision(rx).await.unwrap(),
            ApprovalOutcome::Approved
        );
        assert!(pending.read().unwrap().is_empty());
    }

    #[tokio::test]
    async fn denied_decision_round_trips() {
        let pending = new_pending();
        let rx = register_pending(&pending, "inv-2");
        let tx = pending.write().unwrap().remove("inv-2").unwrap();
        tx.send("denied".to_string()).unwrap();
        assert_eq!(await_decision(rx).await.unwrap(), ApprovalOutcome::Denied);
    }

    #[tokio::test]
    async fn dry_run_decision_round_trips() {
        let pending = new_pending();
        let rx = register_pending(&pending, "inv-3");
        let tx = pending.write().unwrap().remove("inv-3").unwrap();
        tx.send("dry_run".to_string()).unwrap();
        assert_eq!(await_decision(rx).await.unwrap(), ApprovalOutcome::DryRun);
    }

    #[tokio::test]
    async fn channel_closed_yields_expected_error_string() {
        let pending = new_pending();
        let rx = register_pending(&pending, "inv-4");
        // Drop without sending: simulates `cancel_tool_invocation` clearing
        // pending_approvals while a receiver is still parked.
        pending.write().unwrap().clear();
        let err = await_decision(rx).await.unwrap_err();
        assert_eq!(err, "Approval channel closed");
    }

    #[tokio::test]
    async fn unknown_decision_string_falls_through() {
        let pending = new_pending();
        let rx = register_pending(&pending, "inv-5");
        let tx = pending.write().unwrap().remove("inv-5").unwrap();
        tx.send("ignored".to_string()).unwrap();
        match await_decision(rx).await.unwrap() {
            ApprovalOutcome::Other(s) => assert_eq!(s, "ignored"),
            other => panic!("expected Other, got {:?}", other),
        }
    }

    #[test]
    fn outcome_classifier_handles_all_known() {
        assert_eq!(
            ApprovalOutcome::from_decision("approved"),
            ApprovalOutcome::Approved
        );
        assert_eq!(
            ApprovalOutcome::from_decision("denied"),
            ApprovalOutcome::Denied
        );
        assert_eq!(
            ApprovalOutcome::from_decision("dry_run"),
            ApprovalOutcome::DryRun
        );
    }

    /// `cancel_tool_invocation` in `lib.rs` clears `pending_approvals` to
    /// unblock any parked receiver. This test pins the error string the
    /// upstream `JoinHandle::abort` flow relies on.
    #[tokio::test]
    async fn cancel_path_drops_receiver_with_expected_error() {
        let pending = new_pending();
        let rx = register_pending(&pending, "to-cancel");

        // Simulate `cancel_tool_invocation` step 4: clear the map.
        let waiter = tokio::spawn(async move { await_decision(rx).await });
        // Give the spawned task a moment to park on rx.await.
        tokio::task::yield_now().await;
        pending.write().unwrap().clear();

        let result = waiter.await.expect("join");
        assert_eq!(result.unwrap_err(), "Approval channel closed");
    }
}
