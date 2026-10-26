mod common;
use common::*;
use rolvya_domain::ids::*;
use rolvya_persistence_sqlite::*;
use rolvya_plan_validator::{Limits, validate};

#[tokio::test]
async fn activation_is_durable_idempotent_and_separate_from_runtime() {
    let f = Fixture::new().await;
    let receipt = f.activate().await.unwrap();
    assert!(receipt.sequence > 0);
    assert_eq!(receipt, f.activate().await.unwrap());
    let raw = f.raw().await;
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM activation_events")
            .fetch_one(&raw)
            .await
            .unwrap(),
        1
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM task_runtime")
            .fetch_one(&raw)
            .await
            .unwrap(),
        3
    );
    sqlx::query("UPDATE task_runtime SET state='waiting' WHERE task_id='task-a'")
        .execute(&raw)
        .await
        .unwrap();
    let loaded = f
        .store
        .load(&f.content.key, &SyntheticPolicy, Limits::default())
        .await
        .unwrap();
    assert!(loaded.active);
    assert_eq!(loaded.revision, f.revision);
    raw.close().await;
    f.store.close().await;
    let reopened = PlanStore::open(&f.path).await.unwrap();
    assert!(
        reopened
            .load(&f.content.key, &SyntheticPolicy, Limits::default())
            .await
            .unwrap()
            .active
    );
    assert_eq!(
        reopened
            .activate(
                &f.revision,
                &request(),
                &owner(),
                &OwnerPolicy,
                &SyntheticPolicy,
                Limits::default()
            )
            .await
            .unwrap(),
        receipt
    );
}
#[tokio::test]
async fn raw_sql_cannot_change_activated_structure_or_audit() {
    let f = Fixture::new().await;
    f.activate().await.unwrap();
    let raw = f.raw().await;
    for statement in [
        "UPDATE plan_versions SET state='draft',activated_event=NULL",
        "UPDATE plan_versions SET canonical=X'00'",
        "UPDATE plan_versions SET fingerprint=printf('%064d',0)",
        "DELETE FROM plan_versions",
        "INSERT OR REPLACE INTO plan_versions SELECT * FROM plan_versions",
        "UPDATE tasks SET definition='{}'",
        "UPDATE tasks SET task_id='moved' WHERE task_id='task-a'",
        "DELETE FROM tasks",
        "INSERT OR REPLACE INTO tasks SELECT * FROM tasks",
        "UPDATE task_dependencies SET successor_id='z-group'",
        "DELETE FROM task_dependencies",
        "INSERT INTO task_dependencies VALUES ('project','plan',1,'task-b','z-group')",
        "INSERT OR REPLACE INTO task_dependencies SELECT * FROM task_dependencies",
        "UPDATE activation_events SET actor='worker'",
        "DELETE FROM activation_events",
        "UPDATE task_runtime SET task_id='moved'",
        "DELETE FROM task_runtime",
    ] {
        assert!(
            sqlx::query(statement).execute(&raw).await.is_err(),
            "unguarded mutation: {statement}"
        );
    }
    f.store.check_integrity().await.unwrap();
    assert!(
        f.store
            .load(&f.content.key, &SyntheticPolicy, Limits::default())
            .await
            .unwrap()
            .active
    );
}
#[tokio::test]
async fn incomplete_receipt_does_not_falsely_report_success() {
    let f = Fixture::new().await;
    let raw = f.raw().await;
    sqlx::query("INSERT INTO activation_events(project_id,plan_id,version,fingerprint,actor,request_id) VALUES ('project','plan',1,?,'owner','activate-1')").bind(f.revision.fingerprint()).execute(&raw).await.unwrap();
    assert_eq!(f.activate().await.unwrap_err(), StoreError::Integrity);
}
#[tokio::test]
async fn stale_revisions_and_reused_request_ids_fail_closed() {
    let f = Fixture::new().await;
    let mut second = f.content.clone();
    second.key.version = PlanVersion::new(2).unwrap();
    for t in &mut second.tasks {
        t.plan = second.key.clone();
        if let Some(p) = &mut t.parent {
            p.plan = second.key.clone();
        }
        for d in &mut t.dependencies {
            d.plan = second.key.clone();
        }
    }
    let valid = validate(&second, &SyntheticPolicy, Limits::default()).unwrap();
    assert_eq!(
        f.store
            .save_draft(&valid, None, &owner(), &OwnerPolicy)
            .await
            .unwrap_err(),
        StoreError::StaleRevision
    );
    let revision = f
        .store
        .save_draft(&valid, Some(&f.revision), &owner(), &OwnerPolicy)
        .await
        .unwrap();
    assert_eq!(f.activate().await.unwrap_err(), StoreError::StaleRevision);
    f.store
        .activate(
            &revision,
            &request(),
            &owner(),
            &OwnerPolicy,
            &SyntheticPolicy,
            Limits::default(),
        )
        .await
        .unwrap();
    assert_eq!(
        f.activate().await.unwrap_err(),
        StoreError::IdempotencyConflict
    );
    assert_eq!(
        f.store
            .activate(
                &revision,
                &RequestId::new("another").unwrap(),
                &owner(),
                &OwnerPolicy,
                &SyntheticPolicy,
                Limits::default()
            )
            .await
            .unwrap_err(),
        StoreError::AlreadyActive
    );
}
#[tokio::test]
async fn owner_and_current_assignment_policies_are_required() {
    let f = Fixture::new().await;
    assert_eq!(
        f.store
            .activate(
                &f.revision,
                &request(),
                &PrincipalId::new("worker").unwrap(),
                &OwnerPolicy,
                &SyntheticPolicy,
                Limits::default()
            )
            .await
            .unwrap_err(),
        StoreError::Forbidden
    );
    assert_eq!(
        f.store
            .activate(
                &f.revision,
                &request(),
                &owner(),
                &Deny,
                &SyntheticPolicy,
                Limits::default()
            )
            .await
            .unwrap_err(),
        StoreError::Forbidden
    );
    assert!(matches!(
        f.store
            .activate(
                &f.revision,
                &request(),
                &owner(),
                &OwnerPolicy,
                &DenyAssignment,
                Limits::default()
            )
            .await,
        Err(StoreError::InvalidPlan(_))
    ));
    assert!(
        !f.store
            .load(&f.content.key, &SyntheticPolicy, Limits::default())
            .await
            .unwrap()
            .active
    );
    f.activate().await.unwrap();
    // Acknowledgement replay performs no new activation, but still needs Owner authorization.
    assert_eq!(
        f.store
            .activate(
                &f.revision,
                &request(),
                &owner(),
                &OwnerPolicy,
                &DenyAssignment,
                Limits::default()
            )
            .await
            .unwrap()
            .revision,
        f.revision
    );
}
#[tokio::test]
async fn concurrent_activation_and_duplicate_delivery_have_one_effect() {
    let f = Fixture::new().await;
    let second = PlanStore::open(&f.path).await.unwrap();
    let req = request();
    let actor = owner();
    let (a, b) = tokio::join!(
        f.activate(),
        second.activate(
            &f.revision,
            &req,
            &actor,
            &OwnerPolicy,
            &SyntheticPolicy,
            Limits::default()
        )
    );
    assert_eq!(a.unwrap(), b.unwrap());
    let raw = f.raw().await;
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM activation_events")
            .fetch_one(&raw)
            .await
            .unwrap(),
        1
    );
}
#[tokio::test]
async fn statement_failure_rolls_back_runtime_and_activation_then_retry_is_safe() {
    let f = Fixture::new().await;
    let raw = f.raw().await;
    sqlx::query("CREATE TRIGGER fault BEFORE INSERT ON activation_events BEGIN SELECT RAISE(ABORT,'injected failure'); END").execute(&raw).await.unwrap();
    assert!(f.activate().await.is_err());
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM task_runtime")
            .fetch_one(&raw)
            .await
            .unwrap(),
        0
    );
    assert_eq!(
        sqlx::query_scalar::<_, String>("SELECT state FROM plan_versions")
            .fetch_one(&raw)
            .await
            .unwrap(),
        "draft"
    );
    sqlx::query("DROP TRIGGER fault")
        .execute(&raw)
        .await
        .unwrap();
    sqlx::query("CREATE TRIGGER ignored_update BEFORE UPDATE OF state ON plan_versions BEGIN SELECT RAISE(IGNORE); END").execute(&raw).await.unwrap();
    assert!(
        f.activate().await.is_err(),
        "a suppressed state transition must not be acknowledged"
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM activation_events")
            .fetch_one(&raw)
            .await
            .unwrap(),
        0
    );
    sqlx::query("DROP TRIGGER ignored_update")
        .execute(&raw)
        .await
        .unwrap();
    f.activate().await.unwrap();
}
#[tokio::test]
async fn altered_draft_payload_or_relational_graph_cannot_activate() {
    for statement in [
        "UPDATE tasks SET definition='{}'",
        "DELETE FROM task_dependencies",
        "UPDATE tasks SET parent_task_id=NULL",
        "UPDATE plan_versions SET canonical=X'7B7D'",
        "UPDATE plan_versions SET fingerprint=printf('%064d',0)",
    ] {
        let f = Fixture::new().await;
        let raw = f.raw().await;
        sqlx::query(statement).execute(&raw).await.unwrap();
        assert!(
            f.activate().await.is_err(),
            "accepted tampered graph: {statement}"
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT count(*) FROM task_runtime")
                .fetch_one(&raw)
                .await
                .unwrap(),
            0
        );
    }
}
#[tokio::test]
async fn scoped_foreign_keys_and_unique_versions_are_enforced() {
    let f = Fixture::new().await;
    let raw = f.raw().await;
    for statement in [
        "INSERT INTO task_dependencies VALUES ('project','plan',2,'task-a','task-b')",
        "INSERT INTO task_dependencies VALUES ('project','other',1,'task-a','task-b')",
        "INSERT INTO task_dependencies VALUES ('other','plan',1,'task-a','task-b')",
        "INSERT INTO task_dependencies VALUES ('project','plan',1,'missing','task-b')",
        "INSERT INTO task_dependencies SELECT * FROM task_dependencies",
        "INSERT INTO tasks VALUES ('project','plan',1,'bad','missing','executable_leaf','{}')",
        "INSERT INTO tasks VALUES ('project','plan',2,'bad',NULL,'executable_leaf','{}')",
        "INSERT INTO plan_versions SELECT * FROM plan_versions",
    ] {
        assert!(
            sqlx::query(statement).execute(&raw).await.is_err(),
            "constraint missing: {statement}"
        );
    }
}
#[tokio::test]
async fn same_version_changes_require_a_new_version_and_expected_head() {
    let f = Fixture::new().await;
    let valid = validate(&f.content, &SyntheticPolicy, Limits::default()).unwrap();
    assert_eq!(
        f.store
            .save_draft(&valid, None, &owner(), &OwnerPolicy)
            .await
            .unwrap(),
        f.revision
    );
    let mut altered = f.content.clone();
    altered.tasks[0].goal.push_str(" altered");
    let valid = validate(&altered, &SyntheticPolicy, Limits::default()).unwrap();
    assert_eq!(
        f.store
            .save_draft(&valid, Some(&f.revision), &owner(), &OwnerPolicy)
            .await
            .unwrap_err(),
        StoreError::Conflict
    );
    assert_eq!(
        f.store
            .activate(
                &Revision::from_validated(&valid),
                &request(),
                &owner(),
                &OwnerPolicy,
                &SyntheticPolicy,
                Limits::default()
            )
            .await
            .unwrap_err(),
        StoreError::StaleRevision
    );
}

#[tokio::test]
async fn active_plan_is_discoverable_after_restart_without_remembering_version() {
    let f = Fixture::new().await;
    assert_eq!(
        f.store
            .load_active(
                &f.content.key.project_id,
                &SyntheticPolicy,
                Limits::default()
            )
            .await
            .unwrap_err(),
        StoreError::NotFound
    );
    f.activate().await.unwrap();
    f.store.close().await;
    let store = PlanStore::open(&f.path).await.unwrap();
    assert_eq!(
        store
            .load_active(
                &f.content.key.project_id,
                &SyntheticPolicy,
                Limits::default()
            )
            .await
            .unwrap()
            .revision,
        f.revision
    );
}

#[tokio::test]
async fn different_concurrent_activation_keys_cannot_activate_twice() {
    let f = Fixture::new().await;
    let req = RequestId::new("competing-request").unwrap();
    let actor = owner();
    let (a, b) = tokio::join!(
        f.activate(),
        f.store.activate(
            &f.revision,
            &req,
            &actor,
            &OwnerPolicy,
            &SyntheticPolicy,
            Limits::default()
        )
    );
    assert_eq!(usize::from(a.is_ok()) + usize::from(b.is_ok()), 1);
    assert!(a == Err(StoreError::AlreadyActive) || b == Err(StoreError::AlreadyActive));
}
#[tokio::test]
async fn stale_concurrent_draft_writers_cannot_lose_a_revision() {
    let f = Fixture::new().await;
    let make = |v| {
        let mut p = f.content.clone();
        p.key.version = PlanVersion::new(v).unwrap();
        for t in &mut p.tasks {
            t.plan = p.key.clone();
            if let Some(parent) = &mut t.parent {
                parent.plan = p.key.clone();
            }
            for d in &mut t.dependencies {
                d.plan = p.key.clone();
            }
        }
        p
    };
    let a = make(2);
    let b = make(3);
    let va = validate(&a, &SyntheticPolicy, Limits::default()).unwrap();
    let vb = validate(&b, &SyntheticPolicy, Limits::default()).unwrap();
    let actor = owner();
    let (a, b) = tokio::join!(
        f.store
            .save_draft(&va, Some(&f.revision), &actor, &OwnerPolicy),
        f.store
            .save_draft(&vb, Some(&f.revision), &actor, &OwnerPolicy)
    );
    assert_eq!(usize::from(a.is_ok()) + usize::from(b.is_ok()), 1);
    assert!(a == Err(StoreError::StaleRevision) || b == Err(StoreError::StaleRevision));
}
#[tokio::test]
async fn projects_and_request_namespaces_remain_isolated() {
    let f = Fixture::new().await;
    f.activate().await.unwrap();
    let mut other = plan();
    other.key.project_id = ProjectId::new("other-project").unwrap();
    other.tasks[0].plan = other.key.clone();
    let other_owner = PrincipalId::new("other-owner").unwrap();
    f.store
        .create_project(&other.key.project_id, &other_owner)
        .await
        .unwrap();
    let valid = validate(&other, &SyntheticPolicy, Limits::default()).unwrap();
    assert_eq!(
        f.store
            .save_draft(&valid, None, &owner(), &OwnerPolicy)
            .await
            .unwrap_err(),
        StoreError::Forbidden
    );
    let revision = f
        .store
        .save_draft(&valid, None, &other_owner, &OwnerPolicy)
        .await
        .unwrap();
    f.store
        .activate(
            &revision,
            &request(),
            &other_owner,
            &OwnerPolicy,
            &SyntheticPolicy,
            Limits::default(),
        )
        .await
        .unwrap();
    let raw = f.raw().await;
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM activation_events")
            .fetch_one(&raw)
            .await
            .unwrap(),
        2
    );
}

#[tokio::test]
async fn thousand_task_snapshot_round_trip_preserves_the_validated_hash() {
    let f = Fixture::new().await;
    let mut content = plan();
    content.key.version = PlanVersion::new(2).unwrap();
    content.tasks = (0..1000)
        .map(|n| task(&format!("node-{n:04}"), &content.key))
        .collect();
    for n in 1..content.tasks.len() {
        let predecessor = task_ref(&content.tasks[n - 1]);
        content.tasks[n].dependencies.push(predecessor);
    }
    let valid = validate(&content, &SyntheticPolicy, Limits::default()).unwrap();
    let revision = f
        .store
        .save_draft(&valid, Some(&f.revision), &owner(), &OwnerPolicy)
        .await
        .unwrap();
    f.store
        .activate(
            &revision,
            &request(),
            &owner(),
            &OwnerPolicy,
            &SyntheticPolicy,
            Limits::default(),
        )
        .await
        .unwrap();
    let loaded = f
        .store
        .load_active(&content.key.project_id, &SyntheticPolicy, Limits::default())
        .await
        .unwrap();
    assert_eq!(loaded.content.tasks.len(), 1000);
    assert_eq!(loaded.revision, revision);
}
