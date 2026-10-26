mod common;
use common::*;
use rolvya_persistence_sqlite::{StoreError, finance::*};
#[tokio::test]
async fn missing_budget_is_not_spending_authority() {
    let f = Fixture::new().await;
    assert!(matches!(
        f.store.financial_snapshot(&f.content.key.project_id).await,
        Err(FinanceError::Storage(StoreError::NotFound))
    ));
}
use rolvya_domain::{ids::ProjectId, money::*};
use rolvya_persistence_sqlite::{PlanStore, PrincipalId, RequestId};
use std::time::{SystemTime, UNIX_EPOCH};
fn timestamp() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
}
fn usd(value: i64) -> Money {
    Money::new(value, Currency::new("USD").unwrap()).unwrap()
}
fn breakdown(values: [i64; 4]) -> CostBreakdown {
    CostBreakdown {
        ai_api: usd(values[0]),
        compute: usd(values[1]),
        storage: usd(values[2]),
        transfer: usd(values[3]),
    }
}
fn evidence(s: &str) -> EvidenceRef {
    EvidenceRef::new(s).unwrap()
}
fn id(s: &str) -> LiabilityId {
    LiabilityId::new(s).unwrap()
}
fn req(f: &Fixture, key: &str, command: FinancialCommand) -> FinancialRequest {
    FinancialRequest {
        project: f.content.key.project_id.clone(),
        request_id: RequestId::new(key).unwrap(),
        actor: owner(),
        reason: "Synthetic verified action".into(),
        command,
    }
}
fn approval(f: &Fixture, key: &str, version: u64, amount: i64) -> FinancialRequest {
    req(
        f,
        key,
        FinancialCommand::Approve {
            expected_version: version,
            new_ceiling: usd(amount),
            proof: OwnerProof {
                reference: evidence(key),
                verified_at: timestamp(),
            },
        },
    )
}
fn reserve(f: &Fixture, key: &str, amount: [i64; 4]) -> FinancialRequest {
    req(
        f,
        key,
        FinancialCommand::Reserve {
            liability: id(key),
            maximum: breakdown(amount),
            bound_evidence: evidence("synthetic-enforced-cap"),
        },
    )
}
fn report(
    f: &Fixture,
    key: &str,
    liability: &str,
    revision: u64,
    amount: [i64; 4],
    uncertain: bool,
) -> FinancialRequest {
    let mut r = req(
        f,
        key,
        FinancialCommand::Report {
            liability: id(liability),
            expected_revision: revision,
            unreserved: breakdown(amount),
            uncertain,
            evidence: evidence(key),
        },
    );
    r.actor = PrincipalId::new("billing-service").unwrap();
    r
}
fn reconcile(
    f: &Fixture,
    key: &str,
    liability: &str,
    revision: u64,
    amounts: [[i64; 4]; 3],
    state: ReconciliationState,
) -> FinancialRequest {
    let mut r = req(
        f,
        key,
        FinancialCommand::Reconcile {
            liability: id(liability),
            expected_revision: revision,
            settled: breakdown(amounts[0]),
            remaining_hold: breakdown(amounts[1]),
            remaining_provisional: breakdown(amounts[2]),
            state,
            evidence: evidence(key),
        },
    );
    r.actor = PrincipalId::new("billing-service").unwrap();
    r
}
// Synthetic authenticated control-plane evidence; never shipped as a default policy.
struct Verified;
impl FinancialAuthorization for Verified {
    fn permits(&self, current_owner: &PrincipalId, request: &FinancialRequest, _: i64) -> bool {
        match request.command {
            FinancialCommand::Approve { .. } | FinancialCommand::Reserve { .. } => {
                request.actor == *current_owner
            }
            _ => request.actor.as_str() == "billing-service",
        }
    }
}
struct Reject;
impl FinancialAuthorization for Reject {
    fn permits(&self, _: &PrincipalId, _: &FinancialRequest, _: i64) -> bool {
        false
    }
}
struct Permissive;
impl FinancialAuthorization for Permissive {
    fn permits(&self, _: &PrincipalId, _: &FinancialRequest, _: i64) -> bool {
        true
    }
}
async fn funded(amount: i64) -> Fixture {
    let f = Fixture::new().await;
    f.store
        .apply_financial(&approval(&f, "approval-1", 0, amount), &Verified)
        .await
        .unwrap();
    f
}
async fn count(f: &Fixture, table: &str) -> i64 {
    let raw = f.raw().await;
    let value = sqlx::query_scalar(&format!("SELECT count(*) FROM {table}"))
        .fetch_one(&raw)
        .await
        .unwrap();
    raw.close().await;
    value
}
#[tokio::test]
async fn four_categories_exact_headroom_and_idempotent_approval() {
    let f = Fixture::new().await;
    let r = approval(&f, "approve", 0, 100);
    let first = f.store.apply_financial(&r, &Verified).await.unwrap();
    assert_eq!(first, f.store.apply_financial(&r, &Verified).await.unwrap());
    f.store
        .apply_financial(&reserve(&f, "hold", [10, 20, 30, 40]), &Verified)
        .await
        .unwrap();
    let s = f.store.financial_snapshot(&r.project).await.unwrap();
    assert_eq!(s.ceiling, usd(100));
    assert_eq!(s.held, breakdown([10, 20, 30, 40]));
    assert_eq!(s.headroom, usd(0));
    assert_eq!(s.version, 1);
    assert_eq!(
        f.store
            .apply_financial(&reserve(&f, "one-extra", [1, 0, 0, 0]), &Verified)
            .await
            .unwrap_err(),
        FinanceError::InsufficientBudget
    );
    assert_eq!(count(&f, "financial_events").await, 2);
}
#[tokio::test]
async fn worker_cannot_approve_even_when_caller_policy_is_permissive() {
    let f = Fixture::new().await;
    let mut r = approval(&f, "approve", 0, 100);
    r.actor = PrincipalId::new("worker").unwrap();
    assert_eq!(
        f.store.apply_financial(&r, &Permissive).await.unwrap_err(),
        FinanceError::Storage(StoreError::Forbidden)
    );
    assert_eq!(count(&f, "financial_accounts").await, 0);
    assert_eq!(count(&f, "financial_events").await, 0);
}
#[tokio::test]
async fn missing_authentication_revoked_authority_and_denied_callbacks_leave_no_effect() {
    let f = funded(100).await;
    for r in [
        reserve(&f, "hold", [10, 0, 0, 0]),
        report(&f, "bill", "missing", 0, [10, 0, 0, 0], true),
        approval(&f, "increase", 1, 200),
    ] {
        assert_eq!(
            f.store.apply_financial(&r, &Reject).await.unwrap_err(),
            FinanceError::Storage(StoreError::Forbidden)
        );
    }
    let r = reserve(&f, "hold", [10, 0, 0, 0]);
    f.store.apply_financial(&r, &Verified).await.unwrap();
    assert_eq!(
        f.store.apply_financial(&r, &Reject).await.unwrap_err(),
        FinanceError::Storage(StoreError::Forbidden)
    );
    let mut forged = report(&f, "forged", "hold", 1, [0; 4], false);
    forged.actor = PrincipalId::new("worker").unwrap();
    assert_eq!(
        f.store
            .apply_financial(&forged, &Verified)
            .await
            .unwrap_err(),
        FinanceError::Storage(StoreError::Forbidden)
    );
    assert_eq!(count(&f, "financial_events").await, 2);
}
#[tokio::test]
async fn exact_request_binding_rejects_changed_actor_reason_evidence_or_amount() {
    let f = funded(100).await;
    let r = reserve(&f, "hold", [10, 0, 0, 0]);
    f.store.apply_financial(&r, &Verified).await.unwrap();
    let mut variants = vec![r.clone(); 4];
    variants[0].actor = PrincipalId::new("other").unwrap();
    variants[1].reason = "different".into();
    if let FinancialCommand::Reserve { maximum, .. } = &mut variants[2].command {
        *maximum = breakdown([11, 0, 0, 0]);
    }
    if let FinancialCommand::Reserve { bound_evidence, .. } = &mut variants[3].command {
        *bound_evidence = evidence("changed");
    }
    for changed in variants {
        assert_eq!(
            f.store
                .apply_financial(&changed, &Permissive)
                .await
                .unwrap_err(),
            FinanceError::Storage(StoreError::IdempotencyConflict)
        );
    }
    assert_eq!(count(&f, "liability_revisions").await, 1);
}
#[tokio::test]
async fn proof_expiry_future_single_use_and_stale_budget_rejected_atomically() {
    let f = funded(100).await;
    for (key, at) in [
        ("expired", timestamp() - 300),
        ("future", timestamp() + 60),
        ("negative", -1),
    ] {
        let mut r = approval(&f, key, 1, 200);
        if let FinancialCommand::Approve { proof, .. } = &mut r.command {
            proof.verified_at = at;
        }
        assert_eq!(
            f.store.apply_financial(&r, &Verified).await.unwrap_err(),
            FinanceError::ProofExpired
        );
    }
    let mut reused = approval(&f, "reused", 1, 200);
    if let FinancialCommand::Approve { proof, .. } = &mut reused.command {
        proof.reference = evidence("approval-1");
    }
    assert_eq!(
        f.store
            .apply_financial(&reused, &Verified)
            .await
            .unwrap_err(),
        FinanceError::ProofConsumed
    );
    assert_eq!(
        f.store
            .apply_financial(&approval(&f, "stale", 0, 200), &Verified)
            .await
            .unwrap_err(),
        FinanceError::StaleVersion
    );
    assert_eq!(
        f.store
            .apply_financial(&approval(&f, "decrease", 1, 99), &Verified)
            .await
            .unwrap_err(),
        FinanceError::InvalidTransition
    );
    assert_eq!(
        f.store
            .apply_financial(&approval(&f, "same", 1, 100), &Verified)
            .await
            .unwrap_err(),
        FinanceError::InvalidTransition
    );
    assert_eq!(count(&f, "budget_approvals").await, 1);
    let current = approval(&f, "increase", 1, 200);
    f.store.apply_financial(&current, &Verified).await.unwrap();
    assert_eq!(
        f.store
            .financial_snapshot(&current.project)
            .await
            .unwrap()
            .version,
        2
    );
}
#[tokio::test]
async fn concurrent_holds_cannot_spend_the_same_headroom() {
    let f = funded(100).await;
    let a = reserve(&f, "a", [70, 0, 0, 0]);
    let b = reserve(&f, "b", [0, 70, 0, 0]);
    let (a, b) = tokio::join!(
        f.store.apply_financial(&a, &Verified),
        f.store.apply_financial(&b, &Verified)
    );
    assert_eq!(usize::from(a.is_ok()) + usize::from(b.is_ok()), 1);
    assert!(
        matches!(a, Err(FinanceError::InsufficientBudget))
            || matches!(b, Err(FinanceError::InsufficientBudget))
    );
    let s = f
        .store
        .financial_snapshot(&f.content.key.project_id)
        .await
        .unwrap();
    assert_eq!(s.headroom, usd(30));
    assert_eq!(count(&f, "liability_revisions").await, 1);
    assert_eq!(count(&f, "financial_positions").await, 4);
}
#[tokio::test]
async fn duplicate_concurrent_approval_and_hold_have_one_effect() {
    let f = Fixture::new().await;
    let r = approval(&f, "approval", 0, 100);
    let (a, b) = tokio::join!(
        f.store.apply_financial(&r, &Verified),
        f.store.apply_financial(&r, &Verified)
    );
    assert_eq!(a.unwrap(), b.unwrap());
    let r = reserve(&f, "hold", [70, 0, 0, 0]);
    let (a, b) = tokio::join!(
        f.store.apply_financial(&r, &Verified),
        f.store.apply_financial(&r, &Verified)
    );
    assert_eq!(a.unwrap(), b.unwrap());
    assert_eq!(count(&f, "financial_events").await, 2);
    assert_eq!(count(&f, "budget_approvals").await, 1);
}
#[tokio::test]
async fn simultaneous_different_approvals_are_version_fenced() {
    let f = funded(100).await;
    let a = approval(&f, "a", 1, 200);
    let b = approval(&f, "b", 1, 300);
    let (a, b) = tokio::join!(
        f.store.apply_financial(&a, &Verified),
        f.store.apply_financial(&b, &Verified)
    );
    assert_eq!(usize::from(a.is_ok()) + usize::from(b.is_ok()), 1);
    assert!(
        matches!(a, Err(FinanceError::StaleVersion))
            || matches!(b, Err(FinanceError::StaleVersion))
    );
    assert_eq!(count(&f, "budget_approvals").await, 2);
}
#[tokio::test]
async fn settlement_transfers_covered_cost_without_double_counting() {
    let f = funded(200).await;
    f.store
        .apply_financial(&reserve(&f, "hold", [40, 30, 20, 10]), &Verified)
        .await
        .unwrap();
    let r = reconcile(
        &f,
        "invoice",
        "hold",
        1,
        [[15, 10, 5, 5], [25, 20, 15, 5], [0; 4]],
        ReconciliationState::Bounded,
    );
    let first = f.store.apply_financial(&r, &Verified).await.unwrap();
    assert_eq!(first, f.store.apply_financial(&r, &Verified).await.unwrap());
    let s = f.store.financial_snapshot(&r.project).await.unwrap();
    assert_eq!(s.settled, breakdown([15, 10, 5, 5]));
    assert_eq!(s.held, breakdown([25, 20, 15, 5]));
    assert_eq!(s.headroom, usd(100));
    let final_r = reconcile(
        &f,
        "final",
        "hold",
        2,
        [[15, 10, 5, 5], [0; 4], [0; 4]],
        ReconciliationState::Final,
    );
    f.store.apply_financial(&final_r, &Verified).await.unwrap();
    assert_eq!(
        f.store
            .financial_snapshot(&r.project)
            .await
            .unwrap()
            .headroom,
        usd(165)
    );
    assert!(
        f.store
            .financial_liability(&r.project, &id("hold"))
            .await
            .unwrap()
            .finalized
    );
}
#[tokio::test]
async fn provisional_unknown_exposure_blocks_holds_and_reports_cannot_release_it() {
    let f = funded(200).await;
    f.store
        .apply_financial(
            &report(&f, "unknown", "external", 0, [10, 20, 30, 40], true),
            &Verified,
        )
        .await
        .unwrap();
    assert_eq!(
        f.store
            .apply_financial(&reserve(&f, "hold", [1, 0, 0, 0]), &Verified)
            .await
            .unwrap_err(),
        FinanceError::UncertainExposure
    );
    f.store
        .apply_financial(
            &report(&f, "repeat-report", "external", 1, [10, 20, 30, 40], false),
            &Verified,
        )
        .await
        .unwrap();
    assert_eq!(
        f.store
            .financial_snapshot(&f.content.key.project_id)
            .await
            .unwrap()
            .uncertain_liabilities,
        1
    );
    assert_eq!(
        f.store
            .apply_financial(
                &report(&f, "decrease", "external", 2, [0; 4], false),
                &Verified
            )
            .await
            .unwrap_err(),
        FinanceError::InvalidTransition
    );
    let final_r = reconcile(
        &f,
        "reconciled",
        "external",
        2,
        [[5, 10, 15, 20], [0; 4], [0; 4]],
        ReconciliationState::Final,
    );
    f.store.apply_financial(&final_r, &Verified).await.unwrap();
    f.store
        .apply_financial(&reserve(&f, "safe-hold", [100, 0, 0, 0]), &Verified)
        .await
        .unwrap();
    assert_eq!(
        f.store
            .financial_snapshot(&final_r.project)
            .await
            .unwrap()
            .headroom,
        usd(50)
    );
}
#[tokio::test]
async fn zero_usage_delayed_invoice_does_not_release_a_hold() {
    let f = funded(100).await;
    f.store
        .apply_financial(&reserve(&f, "hold", [100, 0, 0, 0]), &Verified)
        .await
        .unwrap();
    f.store
        .apply_financial(
            &report(&f, "no-invoice", "hold", 1, [0; 4], false),
            &Verified,
        )
        .await
        .unwrap();
    let s = f
        .store
        .financial_snapshot(&f.content.key.project_id)
        .await
        .unwrap();
    assert_eq!(s.held, breakdown([100, 0, 0, 0]));
    assert_eq!(s.headroom, usd(0));
}
#[tokio::test]
async fn late_charge_after_finalization_is_visible_and_forces_fresh_reconciliation() {
    let f = funded(200).await;
    f.store
        .apply_financial(&reserve(&f, "hold", [100, 0, 0, 0]), &Verified)
        .await
        .unwrap();
    f.store
        .apply_financial(
            &reconcile(
                &f,
                "final",
                "hold",
                1,
                [[20, 0, 0, 0], [0; 4], [0; 4]],
                ReconciliationState::Final,
            ),
            &Verified,
        )
        .await
        .unwrap();
    let late = reconcile(
        &f,
        "late",
        "hold",
        2,
        [[30, 0, 0, 0], [0; 4], [0; 4]],
        ReconciliationState::Final,
    );
    f.store.apply_financial(&late, &Verified).await.unwrap();
    let s = f.store.financial_snapshot(&late.project).await.unwrap();
    assert_eq!(s.settled, breakdown([30, 0, 0, 0]));
    assert_eq!(s.uncertain_liabilities, 1);
    assert_eq!(s.ceiling, usd(200));
    assert_eq!(
        f.store
            .apply_financial(&reserve(&f, "blocked", [1, 0, 0, 0]), &Verified)
            .await
            .unwrap_err(),
        FinanceError::UncertainExposure
    );
    f.store
        .apply_financial(
            &reconcile(
                &f,
                "verified-late",
                "hold",
                3,
                [[30, 0, 0, 0], [0; 4], [0; 4]],
                ReconciliationState::Final,
            ),
            &Verified,
        )
        .await
        .unwrap();
    assert_eq!(
        f.store
            .financial_snapshot(&late.project)
            .await
            .unwrap()
            .uncertain_liabilities,
        0
    );
}
#[tokio::test]
async fn actual_over_ceiling_charge_is_never_dropped_or_converted_to_approval() {
    let f = funded(100).await;
    f.store
        .apply_financial(&reserve(&f, "hold", [50, 0, 0, 0]), &Verified)
        .await
        .unwrap();
    f.store
        .apply_financial(
            &reconcile(
                &f,
                "overrun",
                "hold",
                1,
                [[120, 0, 0, 0], [0; 4], [0; 4]],
                ReconciliationState::Final,
            ),
            &Verified,
        )
        .await
        .unwrap();
    let s = f
        .store
        .financial_snapshot(&f.content.key.project_id)
        .await
        .unwrap();
    assert_eq!(s.settled.total().unwrap(), usd(120));
    assert_eq!(s.ceiling, usd(100));
    assert_eq!(s.headroom, usd(0));
    assert!(s.over_ceiling);
    f.store
        .apply_financial(
            &reconcile(
                &f,
                "confirmed",
                "hold",
                2,
                [[120, 0, 0, 0], [0; 4], [0; 4]],
                ReconciliationState::Final,
            ),
            &Verified,
        )
        .await
        .unwrap();
    assert_eq!(
        f.store
            .apply_financial(&reserve(&f, "blocked", [1, 0, 0, 0]), &Verified)
            .await
            .unwrap_err(),
        FinanceError::InsufficientBudget
    );
    assert_eq!(count(&f, "budget_approvals").await, 1);
}
#[tokio::test]
async fn stale_reconciliation_and_reused_billing_evidence_cannot_change_balances() {
    let f = funded(100).await;
    f.store
        .apply_financial(&reserve(&f, "hold", [50, 0, 0, 0]), &Verified)
        .await
        .unwrap();
    let r = reconcile(
        &f,
        "invoice",
        "hold",
        1,
        [[10, 0, 0, 0], [40, 0, 0, 0], [0; 4]],
        ReconciliationState::Bounded,
    );
    f.store.apply_financial(&r, &Verified).await.unwrap();
    let mut duplicate = r.clone();
    duplicate.request_id = RequestId::new("another-key").unwrap();
    assert_eq!(
        f.store
            .apply_financial(&duplicate, &Verified)
            .await
            .unwrap_err(),
        FinanceError::Storage(StoreError::IdempotencyConflict)
    );
    let stale = reconcile(
        &f,
        "stale",
        "hold",
        1,
        [[20, 0, 0, 0], [30, 0, 0, 0], [0; 4]],
        ReconciliationState::Bounded,
    );
    assert_eq!(
        f.store
            .apply_financial(&stale, &Verified)
            .await
            .unwrap_err(),
        FinanceError::StaleVersion
    );
    assert_eq!(count(&f, "liability_revisions").await, 2);
}
#[tokio::test]
async fn concurrent_invoices_are_revision_fenced_and_duplicate_delivery_is_idempotent() {
    let f = funded(100).await;
    f.store
        .apply_financial(&reserve(&f, "hold", [50, 0, 0, 0]), &Verified)
        .await
        .unwrap();
    let a = reconcile(
        &f,
        "a",
        "hold",
        1,
        [[10, 0, 0, 0], [40, 0, 0, 0], [0; 4]],
        ReconciliationState::Bounded,
    );
    let mut b = a.clone();
    b.request_id = RequestId::new("b").unwrap();
    if let FinancialCommand::Reconcile { evidence, .. } = &mut b.command {
        *evidence = EvidenceRef::new("b").unwrap();
    }
    let (a, b) = tokio::join!(
        f.store.apply_financial(&a, &Verified),
        f.store.apply_financial(&b, &Verified)
    );
    assert_eq!(usize::from(a.is_ok()) + usize::from(b.is_ok()), 1);
    assert!(
        matches!(a, Err(FinanceError::StaleVersion))
            || matches!(b, Err(FinanceError::StaleVersion))
    );
    assert_eq!(count(&f, "liability_revisions").await, 2);
}
#[tokio::test]
async fn invalid_refund_hold_increase_final_state_and_missing_liability_are_rejected() {
    let f = funded(100).await;
    f.store
        .apply_financial(&reserve(&f, "hold", [50, 0, 0, 0]), &Verified)
        .await
        .unwrap();
    f.store
        .apply_financial(
            &reconcile(
                &f,
                "partial",
                "hold",
                1,
                [[10, 0, 0, 0], [40, 0, 0, 0], [0; 4]],
                ReconciliationState::Bounded,
            ),
            &Verified,
        )
        .await
        .unwrap();
    for (key, values, state) in [
        (
            "refund",
            [[9, 0, 0, 0], [40, 0, 0, 0], [0; 4]],
            ReconciliationState::Bounded,
        ),
        (
            "increase",
            [[10, 0, 0, 0], [41, 0, 0, 0], [0; 4]],
            ReconciliationState::Bounded,
        ),
        (
            "final-hold",
            [[10, 0, 0, 0], [40, 0, 0, 0], [0; 4]],
            ReconciliationState::Final,
        ),
        (
            "final-provisional",
            [[10, 0, 0, 0], [0; 4], [1, 0, 0, 0]],
            ReconciliationState::Final,
        ),
    ] {
        assert_eq!(
            f.store
                .apply_financial(&reconcile(&f, key, "hold", 2, values, state), &Verified)
                .await
                .unwrap_err(),
            FinanceError::InvalidTransition
        );
    }
    assert_eq!(
        f.store
            .apply_financial(
                &reconcile(
                    &f,
                    "missing",
                    "missing",
                    0,
                    [[0; 4]; 3],
                    ReconciliationState::Final
                ),
                &Verified
            )
            .await
            .unwrap_err(),
        FinanceError::Storage(StoreError::NotFound)
    );
    let mut reused = reserve(&f, "different-key", [50, 0, 0, 0]);
    if let FinancialCommand::Reserve { liability, .. } = &mut reused.command {
        *liability = id("hold");
    }
    assert_eq!(
        f.store
            .apply_financial(&reused, &Verified)
            .await
            .unwrap_err(),
        FinanceError::InvalidTransition
    );
}
#[tokio::test]
async fn project_currency_and_owner_scope_are_enforced() {
    let f = funded(100).await;
    let other = ProjectId::new("other").unwrap();
    f.store
        .create_project(&other, &PrincipalId::new("other-owner").unwrap())
        .await
        .unwrap();
    let mut wrong = approval(&f, "other-approval", 0, 100);
    wrong.project = other.clone();
    assert_eq!(
        f.store
            .apply_financial(&wrong, &Permissive)
            .await
            .unwrap_err(),
        FinanceError::Storage(StoreError::Forbidden)
    );
    let mut r = reserve(&f, "eur", [1, 0, 0, 0]);
    if let FinancialCommand::Reserve { maximum, .. } = &mut r.command {
        maximum.ai_api = Money::new(1, Currency::new("EUR").unwrap()).unwrap();
    }
    assert_eq!(
        f.store.apply_financial(&r, &Verified).await.unwrap_err(),
        FinanceError::CurrencyMismatch
    );
    let mut r = approval(&f, "eur-approval", 1, 200);
    if let FinancialCommand::Approve { new_ceiling, .. } = &mut r.command {
        *new_ceiling = Money::new(200, Currency::new("EUR").unwrap()).unwrap();
    }
    assert_eq!(
        f.store.apply_financial(&r, &Verified).await.unwrap_err(),
        FinanceError::CurrencyMismatch
    );
    assert_eq!(
        f.store
            .financial_liability(&other, &id("hold"))
            .await
            .unwrap_err(),
        FinanceError::Storage(StoreError::NotFound)
    );
    assert_eq!(count(&f, "financial_events").await, 1);
}
#[tokio::test]
async fn malformed_requests_zero_holds_and_version_overflow_leave_no_events() {
    let f = funded(100).await;
    for reason in ["".into(), " ".into(), "a\nb".into(), "x".repeat(513)] {
        let mut r = reserve(&f, "bad", [1, 0, 0, 0]);
        r.reason = reason;
        assert_eq!(
            f.store.apply_financial(&r, &Verified).await.unwrap_err(),
            FinanceError::InvalidInput
        );
    }
    assert_eq!(
        f.store
            .apply_financial(&reserve(&f, "zero", [0; 4]), &Verified)
            .await
            .unwrap_err(),
        FinanceError::InvalidInput
    );
    assert_eq!(
        f.store
            .apply_financial(&approval(&f, "overflow", u64::MAX, 200), &Verified)
            .await
            .unwrap_err(),
        FinanceError::Arithmetic
    );
    assert_eq!(
        f.store
            .apply_financial(&reserve(&f, "overflow", [i64::MAX, 1, 0, 0]), &Verified)
            .await
            .unwrap_err(),
        FinanceError::Arithmetic
    );
    assert_eq!(count(&f, "financial_events").await, 1);
}
#[tokio::test]
async fn ledger_history_is_append_only_including_sql_replace() {
    let f = funded(100).await;
    f.store
        .apply_financial(&reserve(&f, "hold", [50, 0, 0, 0]), &Verified)
        .await
        .unwrap();
    let raw = f.raw().await;
    for table in [
        "financial_accounts",
        "financial_events",
        "budget_approvals",
        "liability_revisions",
        "financial_positions",
    ] {
        for statement in [
            format!("DELETE FROM {table}"),
            format!("UPDATE {table} SET rowid=rowid"),
            format!("INSERT OR REPLACE INTO {table} SELECT * FROM {table}"),
        ] {
            assert!(
                sqlx::query(&statement).execute(&raw).await.is_err(),
                "{statement}"
            );
        }
    }
    let s = f
        .store
        .financial_snapshot(&f.content.key.project_id)
        .await
        .unwrap();
    assert_eq!(s.headroom, usd(50));
}
#[tokio::test]
async fn fault_at_each_financial_write_rolls_back_every_previous_write() {
    for target in ["financial_events", "budget_approvals", "financial_accounts"] {
        let f = Fixture::new().await;
        let raw = f.raw().await;
        sqlx::query(&format!("CREATE TRIGGER injected BEFORE INSERT ON {target} BEGIN SELECT RAISE(ABORT,'synthetic failure'); END")).execute(&raw).await.unwrap();
        assert!(
            f.store
                .apply_financial(&approval(&f, "approve", 0, 100), &Verified)
                .await
                .is_err()
        );
        assert_eq!(count(&f, "financial_events").await, 0);
        assert_eq!(count(&f, "financial_accounts").await, 0);
    }
    for target in [
        "financial_events",
        "liability_revisions",
        "financial_positions",
    ] {
        for mode in ["ABORT", "IGNORE"] {
            let f = funded(100).await;
            let raw = f.raw().await;
            let raise = if mode == "IGNORE" {
                "RAISE(IGNORE)"
            } else {
                "RAISE(ABORT,'synthetic failure')"
            };
            let condition = if target == "financial_positions" {
                "WHEN NEW.category='storage'"
            } else {
                ""
            };
            sqlx::query(&format!("CREATE TRIGGER injected BEFORE INSERT ON {target} {condition} BEGIN SELECT {raise}; END")).execute(&raw).await.unwrap();
            assert!(
                f.store
                    .apply_financial(&reserve(&f, "hold", [10, 20, 30, 40]), &Verified)
                    .await
                    .is_err(),
                "{target}/{mode}"
            );
            assert_eq!(count(&f, "financial_events").await, 1);
            assert_eq!(count(&f, "liability_revisions").await, 0);
            assert_eq!(count(&f, "financial_positions").await, 0);
        }
    }
}
#[tokio::test]
async fn current_policy_revocation_just_before_commit_rolls_back_approval_and_hold() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    struct Revoke(AtomicUsize);
    impl FinancialAuthorization for Revoke {
        fn permits(&self, _: &PrincipalId, _: &FinancialRequest, _: i64) -> bool {
            self.0.fetch_add(1, Ordering::SeqCst) == 0
        }
    }
    let f = Fixture::new().await;
    assert_eq!(
        f.store
            .apply_financial(
                &approval(&f, "approve", 0, 100),
                &Revoke(AtomicUsize::new(0))
            )
            .await
            .unwrap_err(),
        FinanceError::Storage(StoreError::Forbidden)
    );
    assert_eq!(count(&f, "financial_accounts").await, 0);
    assert_eq!(count(&f, "financial_events").await, 0);
    f.store
        .apply_financial(&approval(&f, "approve", 0, 100), &Verified)
        .await
        .unwrap();
    assert!(
        f.store
            .apply_financial(
                &reserve(&f, "hold", [10, 0, 0, 0]),
                &Revoke(AtomicUsize::new(0))
            )
            .await
            .is_err()
    );
    assert_eq!(count(&f, "financial_events").await, 1);
}
#[tokio::test]
async fn aggregate_integer_overflow_preserves_reported_charges_and_blocks_holds() {
    let f = funded(i64::MAX).await;
    for (key, liability) in [("first", "a"), ("second", "b")] {
        f.store
            .apply_financial(
                &report(&f, key, liability, 0, [i64::MAX, 0, 0, 0], false),
                &Verified,
            )
            .await
            .unwrap();
    }
    assert_eq!(
        f.store
            .financial_snapshot(&f.content.key.project_id)
            .await
            .unwrap_err(),
        FinanceError::Arithmetic
    );
    assert_eq!(
        f.store
            .apply_financial(&reserve(&f, "blocked", [1, 0, 0, 0]), &Verified)
            .await
            .unwrap_err(),
        FinanceError::Arithmetic
    );
    assert_eq!(count(&f, "liability_revisions").await, 2);
}
#[tokio::test]
async fn bounded_history_pagination_and_snapshot_restore_preserve_receipts() {
    let f = funded(100).await;
    let r = reserve(&f, "hold", [50, 0, 0, 0]);
    let receipt = f.store.apply_financial(&r, &Verified).await.unwrap();
    let first = f.store.financial_history(&r.project, 0, 1).await.unwrap();
    assert_eq!(first.len(), 1);
    let second = f
        .store
        .financial_history(&r.project, first[0].sequence, 1)
        .await
        .unwrap();
    assert_eq!(second[0].sequence, receipt.sequence);
    assert!(second[0].request_json.contains("\"format\":1"));
    for (after, limit) in [(-1, 1), (0, 0), (0, 101)] {
        assert_eq!(
            f.store
                .financial_history(&r.project, after, limit)
                .await
                .unwrap_err(),
            FinanceError::InvalidInput
        );
    }
    let path = f.directory.path().join("finance-snapshot.sqlite");
    f.store.snapshot(&path).await.unwrap();
    let restored = PlanStore::open(path).await.unwrap();
    assert_eq!(
        restored.apply_financial(&r, &Verified).await.unwrap(),
        receipt
    );
    assert_eq!(
        restored
            .financial_snapshot(&r.project)
            .await
            .unwrap()
            .headroom,
        usd(50)
    );
    restored.check_integrity().await.unwrap();
}

#[tokio::test]
async fn zero_ceiling_is_explicit_and_does_not_permit_positive_holds() {
    let f = funded(0).await;
    assert_eq!(
        f.store
            .financial_snapshot(&f.content.key.project_id)
            .await
            .unwrap()
            .ceiling,
        usd(0)
    );
    assert_eq!(
        f.store
            .apply_financial(&reserve(&f, "hold", [1, 0, 0, 0]), &Verified)
            .await
            .unwrap_err(),
        FinanceError::InsufficientBudget
    );
}
#[tokio::test]
async fn unauthorized_reconciliation_cannot_release_held_funds() {
    let f = funded(100).await;
    f.store
        .apply_financial(&reserve(&f, "hold", [100, 0, 0, 0]), &Verified)
        .await
        .unwrap();
    let mut r = reconcile(
        &f,
        "release",
        "hold",
        1,
        [[0; 4]; 3],
        ReconciliationState::Final,
    );
    r.actor = owner();
    assert_eq!(
        f.store.apply_financial(&r, &Verified).await.unwrap_err(),
        FinanceError::Storage(StoreError::Forbidden)
    );
    assert_eq!(
        f.store
            .financial_snapshot(&r.project)
            .await
            .unwrap()
            .held
            .total()
            .unwrap(),
        usd(100)
    );
}
#[tokio::test]
async fn proof_consumption_is_global_and_project_holds_are_isolated() {
    let f = funded(100).await;
    let other = ProjectId::new("other").unwrap();
    f.store.create_project(&other, &owner()).await.unwrap();
    let mut other_approval = approval(&f, "other-approval", 0, 200);
    other_approval.project = other.clone();
    if let FinancialCommand::Approve { proof, .. } = &mut other_approval.command {
        proof.reference = evidence("approval-1");
    }
    assert_eq!(
        f.store
            .apply_financial(&other_approval, &Verified)
            .await
            .unwrap_err(),
        FinanceError::ProofConsumed
    );
    if let FinancialCommand::Approve { proof, .. } = &mut other_approval.command {
        proof.reference = evidence("other-proof");
    }
    f.store
        .apply_financial(&other_approval, &Verified)
        .await
        .unwrap();
    let mut hold = reserve(&f, "same-id", [200, 0, 0, 0]);
    hold.project = other.clone();
    f.store.apply_financial(&hold, &Verified).await.unwrap();
    f.store
        .apply_financial(&reserve(&f, "same-id", [100, 0, 0, 0]), &Verified)
        .await
        .unwrap();
    assert_eq!(
        f.store
            .financial_snapshot(&other)
            .await
            .unwrap()
            .held
            .total()
            .unwrap(),
        usd(200)
    );
    assert_eq!(
        f.store
            .financial_snapshot(&f.content.key.project_id)
            .await
            .unwrap()
            .held
            .total()
            .unwrap(),
        usd(100)
    );
}
#[tokio::test]
async fn reconciliation_faults_preserve_all_old_balances() {
    for mode in ["ABORT", "IGNORE"] {
        let f = funded(100).await;
        f.store
            .apply_financial(&reserve(&f, "hold", [10, 20, 30, 40]), &Verified)
            .await
            .unwrap();
        let before = f
            .store
            .financial_snapshot(&f.content.key.project_id)
            .await
            .unwrap();
        let raw = f.raw().await;
        let raise = if mode == "IGNORE" {
            "RAISE(IGNORE)"
        } else {
            "RAISE(ABORT,'synthetic failure')"
        };
        sqlx::query(&format!("CREATE TRIGGER injected BEFORE INSERT ON financial_positions WHEN NEW.category='storage' BEGIN SELECT {raise}; END")).execute(&raw).await.unwrap();
        assert!(
            f.store
                .apply_financial(
                    &reconcile(
                        &f,
                        "invoice",
                        "hold",
                        1,
                        [[5, 5, 5, 5], [0; 4], [0; 4]],
                        ReconciliationState::Final
                    ),
                    &Verified
                )
                .await
                .is_err()
        );
        assert_eq!(
            f.store
                .financial_snapshot(&f.content.key.project_id)
                .await
                .unwrap(),
            before
        );
        assert_eq!(count(&f, "financial_events").await, 2);
        sqlx::query("DROP TRIGGER injected")
            .execute(&raw)
            .await
            .unwrap();
        f.store
            .apply_financial(
                &reconcile(
                    &f,
                    "invoice",
                    "hold",
                    1,
                    [[5, 5, 5, 5], [0; 4], [0; 4]],
                    ReconciliationState::Final,
                ),
                &Verified,
            )
            .await
            .unwrap();
    }
}
#[tokio::test]
async fn financial_busy_timeout_preserves_headroom_while_wal_readers_continue() {
    use std::time::{Duration, Instant};
    let f = funded(100).await;
    let raw = f.raw().await;
    let tx = raw.begin_with("BEGIN IMMEDIATE").await.unwrap();
    assert_eq!(
        f.store
            .financial_snapshot(&f.content.key.project_id)
            .await
            .unwrap()
            .headroom,
        usd(100)
    );
    let start = Instant::now();
    assert_eq!(
        tokio::time::timeout(
            Duration::from_secs(12),
            f.store
                .apply_financial(&reserve(&f, "hold", [100, 0, 0, 0]), &Verified)
        )
        .await
        .unwrap()
        .unwrap_err(),
        FinanceError::Storage(StoreError::Busy)
    );
    assert!(start.elapsed() >= Duration::from_secs(4));
    tx.rollback().await.unwrap();
    assert_eq!(count(&f, "financial_events").await, 1);
    f.store
        .apply_financial(&reserve(&f, "hold", [100, 0, 0, 0]), &Verified)
        .await
        .unwrap();
}
#[tokio::test]
async fn financial_process_exit_before_and_after_commit_is_recoverable() {
    if let Ok(mode) = std::env::var("ROLVYA_FINANCE_TEST_MODE") {
        let path = std::env::var("ROLVYA_FINANCE_TEST_DB").unwrap();
        if mode == "uncommitted" {
            use sqlx::sqlite::SqliteConnectOptions;
            let pool = sqlx::SqlitePool::connect_with(
                SqliteConnectOptions::new()
                    .filename(&path)
                    .foreign_keys(true),
            )
            .await
            .unwrap();
            let mut tx = pool.begin_with("BEGIN IMMEDIATE").await.unwrap();
            let seq:i64=sqlx::query_scalar("INSERT INTO financial_events(project_id,request_id,actor,recorded_at,kind,payload) VALUES ('project','interrupted','owner',1,'reserve','{}') RETURNING sequence").fetch_one(&mut *tx).await.unwrap();
            sqlx::query("INSERT INTO liability_revisions VALUES (?,'project','interrupted',1,0,0)")
                .bind(seq)
                .execute(&mut *tx)
                .await
                .unwrap();
            for category in ["ai_api", "compute", "storage", "transfer"] {
                sqlx::query("INSERT INTO financial_positions VALUES (?,?,0,25,0)")
                    .bind(seq)
                    .bind(category)
                    .execute(&mut *tx)
                    .await
                    .unwrap();
            }
            std::process::exit(17);
        }
        let store = PlanStore::open(path).await.unwrap();
        let r = FinancialRequest {
            project: ProjectId::new("project").unwrap(),
            request_id: RequestId::new("hold").unwrap(),
            actor: owner(),
            reason: "Synthetic verified action".into(),
            command: FinancialCommand::Reserve {
                liability: id("hold"),
                maximum: breakdown([50, 0, 0, 0]),
                bound_evidence: evidence("synthetic-enforced-cap"),
            },
        };
        store.apply_financial(&r, &Verified).await.unwrap();
        std::process::exit(17);
    }
    let f = funded(100).await;
    for mode in ["uncommitted", "committed"] {
        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "financial_process_exit_before_and_after_commit_is_recoverable",
                "--nocapture",
            ])
            .env("ROLVYA_FINANCE_TEST_MODE", mode)
            .env("ROLVYA_FINANCE_TEST_DB", &f.path)
            .status()
            .unwrap();
        assert_eq!(status.code(), Some(17));
        let snapshot = f
            .store
            .financial_snapshot(&f.content.key.project_id)
            .await
            .unwrap();
        assert_eq!(
            snapshot.held.total().unwrap(),
            usd(if mode == "committed" { 50 } else { 0 })
        );
        assert_eq!(
            count(&f, "financial_events").await,
            if mode == "committed" { 2 } else { 1 }
        );
    }
    let r = reserve(&f, "hold", [50, 0, 0, 0]);
    let receipt = f.store.apply_financial(&r, &Verified).await.unwrap();
    assert_eq!(
        f.store.apply_financial(&r, &Verified).await.unwrap(),
        receipt
    );
    f.store.check_integrity().await.unwrap();
}

#[tokio::test]
async fn incomplete_financial_receipt_never_reports_success() {
    let f = funded(100).await;
    let r = reserve(&f, "incomplete", [50, 0, 0, 0]);
    let raw = f.raw().await;
    let payload = serde_json::to_string(&serde_json::json!({"format":1,"request":r})).unwrap();
    sqlx::query("INSERT INTO financial_events(project_id,request_id,actor,recorded_at,kind,payload) VALUES (?,?,?,?,?,?)").bind(r.project.as_str()).bind(r.request_id.as_str()).bind(r.actor.as_str()).bind(timestamp()).bind("reserve").bind(payload).execute(&raw).await.unwrap();
    assert_eq!(
        f.store.apply_financial(&r, &Verified).await.unwrap_err(),
        FinanceError::Storage(StoreError::Integrity)
    );
}

#[tokio::test]
async fn settled_cost_cannot_leave_the_same_money_available_in_its_hold() {
    let f = funded(100).await;
    f.store
        .apply_financial(&reserve(&f, "hold", [50, 0, 0, 0]), &Verified)
        .await
        .unwrap();
    let r = reconcile(
        &f,
        "double-count",
        "hold",
        1,
        [[40, 0, 0, 0], [50, 0, 0, 0], [0; 4]],
        ReconciliationState::Bounded,
    );
    assert_eq!(
        f.store.apply_financial(&r, &Verified).await.unwrap_err(),
        FinanceError::InvalidTransition
    );
    assert_eq!(count(&f, "liability_revisions").await, 1);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn many_concurrent_sendable_requests_never_exceed_the_exact_ceiling() {
    let f = funded(100).await;
    let mut work = tokio::task::JoinSet::new();
    for i in 0..32 {
        let request = reserve(&f, &format!("hold-{i}"), [1, 2, 3, 1]);
        let store = f.store.clone();
        work.spawn(async move { store.apply_financial(&request, &Verified).await });
    }
    let mut accepted = 0;
    while let Some(result) = work.join_next().await {
        match result.unwrap() {
            Ok(_) => accepted += 1,
            Err(e) => assert_eq!(e, FinanceError::InsufficientBudget),
        }
    }
    assert_eq!(accepted, 14);
    let snapshot = f
        .store
        .financial_snapshot(&f.content.key.project_id)
        .await
        .unwrap();
    assert_eq!(snapshot.held.total().unwrap(), usd(98));
    assert_eq!(snapshot.headroom, usd(2));
    assert!(!snapshot.over_ceiling);
    assert_eq!(count(&f, "financial_events").await, 15);
}
