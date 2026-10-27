use crate::*;
use crate::{admission::*, finance::*, orchestration::*, worker_control::*, worker_simulation::*};
use rolvya_domain::{ids::*, money::*, plan::*, worker::*};
use rolvya_plan_validator::{AssignmentDenial, AssignmentPolicy, Limits};
#[path = "../common/mod.rs"]
pub(crate) mod common;
#[path = "../common/synthetic.rs"]
pub(crate) mod synthetic;
use common::*;
use synthetic::*;
mod adversarial;
mod backup;
mod crashes;
mod faults;
mod inventory;
mod oracle;
mod storage;
use faults::*;
use oracle::*;

#[tokio::test]
async fn ignored_delivery_update_is_not_reported_as_progress() {
    let f = fixture(100).await;
    let a = Authority::default();
    let co = open(&f, &a).await;
    let clock = DeterministicClock::new(T).unwrap();
    let raw = f.raw().await;
    sqlx::query("CREATE TRIGGER fault_delivery BEFORE UPDATE ON synthetic_worker_events BEGIN SELECT RAISE(IGNORE); END").execute(&raw).await.unwrap();
    let result = co.pass(&a, &clock, PassLimits::default()).await;
    assert!(
        result.is_err(),
        "ignored delivery write was reported as successful progress"
    );
    assert_eq!(count(&f, "task_acceptances").await, 0);
    sqlx::query("DROP TRIGGER fault_delivery")
        .execute(&raw)
        .await
        .unwrap();
    assert_eq!(
        co.run(&a, &clock, RunLimits::default())
            .await
            .unwrap()
            .status,
        RunStatus::Complete
    );
    assert_eq!(count(&f, "synthetic_operations").await, 3);
}
