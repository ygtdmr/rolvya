mod common;
use common::*;
use rolvya_domain::ids::ProjectId;
use rolvya_persistence_sqlite::finance::{
    EvidenceRef, FinancialCommand, FinancialRequest, OwnerProof,
};
use rolvya_persistence_sqlite::*;
use rolvya_plan_validator::Limits;
use sqlx::migrate::Migrator;

#[tokio::test]
async fn migrations_replay_reopen_and_consistent_snapshot_restore() {
    let f = Fixture::new().await;
    let receipt = f.activate().await.unwrap();
    let snapshot = f.directory.path().join("snapshot.sqlite");
    f.store.snapshot(&snapshot).await.unwrap();
    assert_eq!(
        f.store.snapshot(&snapshot).await.unwrap_err(),
        StoreError::SnapshotExists
    );
    let restored = PlanStore::open(&snapshot).await.unwrap();
    restored.check_integrity().await.unwrap();
    let plan = restored
        .load(&f.content.key, &SyntheticPolicy, Limits::default())
        .await
        .unwrap();
    assert!(plan.active);
    assert_eq!(plan.revision, f.revision);
    assert_eq!(
        restored
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
    restored.close().await;
    let reopened = PlanStore::open(&snapshot).await.unwrap();
    reopened.check_integrity().await.unwrap();
    let raw = f.raw().await;
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM _sqlx_migrations WHERE success=1")
            .fetch_one(&raw)
            .await
            .unwrap(),
        16
    );
    assert_eq!(
        sqlx::query_scalar::<_, String>("SELECT sqlite_version()")
            .fetch_one(&raw)
            .await
            .unwrap()
            .split('.')
            .next(),
        Some("3")
    );
}
#[tokio::test]
async fn checksum_mismatch_and_unknown_migration_versions_fail_without_resetting_data() {
    for alteration in [
        "UPDATE _sqlx_migrations SET checksum=X'00'",
        "UPDATE _sqlx_migrations SET version=999 WHERE version=1",
    ] {
        let f = Fixture::new().await;
        let raw = f.raw().await;
        sqlx::query(alteration).execute(&raw).await.unwrap();
        raw.close().await;
        f.store.close().await;
        assert_eq!(
            PlanStore::open(&f.path).await.unwrap_err(),
            StoreError::Migration
        );
        let raw = f.raw().await;
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT count(*) FROM tasks")
                .fetch_one(&raw)
                .await
                .unwrap(),
            3
        );
    }
}
#[tokio::test]
async fn unknown_schema_is_not_silently_adopted() {
    let f = Fixture::new().await;
    f.store.close().await;
    let raw = f.raw().await;
    sqlx::query("DROP TABLE _sqlx_migrations")
        .execute(&raw)
        .await
        .unwrap();
    assert_eq!(
        sqlx::query_scalar::<_, String>("PRAGMA journal_mode=DELETE")
            .fetch_one(&raw)
            .await
            .unwrap(),
        "delete"
    );
    raw.close().await;
    assert_eq!(
        PlanStore::open(&f.path).await.unwrap_err(),
        StoreError::UnknownDatabase
    );
    let raw = f.raw().await;
    assert_eq!(
        sqlx::query_scalar::<_, String>("PRAGMA journal_mode")
            .fetch_one(&raw)
            .await
            .unwrap(),
        "delete",
        "refusing an unrelated schema must not convert its journal"
    );
}

#[tokio::test]
async fn failed_migration_rolls_back_ddl_and_data() {
    let f = Fixture::new().await;
    let raw = f.raw().await;
    let scripts = tempfile::tempdir().unwrap();
    std::fs::write(scripts.path().join("0017_injected_failure.sql"),"CREATE TABLE should_rollback(value TEXT); INSERT INTO should_rollback VALUES ('temporary'); SELECT absent_column FROM absent_table;").unwrap();
    // Replay real migrations, then verify failing v17 executes and rolls back atomically.
    std::fs::write(
        scripts.path().join("0001_plan_store.sql"),
        include_str!("../migrations/0001_plan_store.sql"),
    )
    .unwrap();
    std::fs::write(
        scripts.path().join("0002_financial_ledger.sql"),
        include_str!("../migrations/0002_financial_ledger.sql"),
    )
    .unwrap();
    std::fs::write(
        scripts.path().join("0003_scheduler_admission.sql"),
        include_str!("../migrations/0003_scheduler_admission.sql"),
    )
    .unwrap();
    std::fs::write(
        scripts.path().join("0004_durable_worker_control.sql"),
        include_str!("../migrations/0004_durable_worker_control.sql"),
    )
    .unwrap();
    std::fs::write(
        scripts.path().join("0005_synthetic_orchestration.sql"),
        include_str!("../migrations/0005_synthetic_orchestration.sql"),
    )
    .unwrap();
    std::fs::write(
        scripts.path().join("0006_controller_key_bootstrap.sql"),
        include_str!("../migrations/0006_controller_key_bootstrap.sql"),
    )
    .unwrap();
    std::fs::write(
        scripts.path().join("0007_secret_broker.sql"),
        include_str!("../migrations/0007_secret_broker.sql"),
    )
    .unwrap();
    std::fs::write(
        scripts.path().join("0008_human_primary_authentication.sql"),
        include_str!("../migrations/0008_human_primary_authentication.sql"),
    )
    .unwrap();
    std::fs::write(
        scripts.path().join("0009_human_sessions_totp.sql"),
        include_str!("../migrations/0009_human_sessions_totp.sql"),
    )
    .unwrap();
    std::fs::write(
        scripts.path().join("0010_human_owner_authorization.sql"),
        include_str!("../migrations/0010_human_owner_authorization.sql"),
    )
    .unwrap();
    std::fs::write(
        scripts.path().join("0011_worker_credential_authority.sql"),
        include_str!("../migrations/0011_worker_credential_authority.sql"),
    )
    .unwrap();
    std::fs::write(
        scripts
            .path()
            .join("0012_provider_account_entitlements.sql"),
        include_str!("../migrations/0012_provider_account_entitlements.sql"),
    )
    .unwrap();
    std::fs::write(
        scripts.path().join("0013_aws_fargate_identity.sql"),
        include_str!("../migrations/0013_aws_fargate_identity.sql"),
    )
    .unwrap();
    std::fs::write(
        scripts.path().join("0014_aws_protected_bootstrap.sql"),
        include_str!("../migrations/0014_aws_protected_bootstrap.sql"),
    )
    .unwrap();
    std::fs::write(
        scripts.path().join("0015_aws_execution_lifecycle.sql"),
        include_str!("../migrations/0015_aws_execution_lifecycle.sql"),
    )
    .unwrap();
    std::fs::write(
        scripts.path().join("0016_github_trusted_publication.sql"),
        include_str!("../migrations/0016_github_trusted_publication.sql"),
    )
    .unwrap();
    let migrator = Migrator::new(scripts.path()).await.unwrap();
    let error = migrator.run(&raw).await.unwrap_err();
    assert!(
        matches!(error, sqlx::migrate::MigrateError::ExecuteMigration(_, 17)),
        "must execute the injected next migration, not reject migration metadata"
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM sqlite_schema WHERE name='should_rollback'"
        )
        .fetch_one(&raw)
        .await
        .unwrap(),
        0
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM tasks")
            .fetch_one(&raw)
            .await
            .unwrap(),
        3
    );
    f.store.close().await;
    raw.close().await;
    assert!(PlanStore::open(&f.path).await.is_ok());
}
#[tokio::test]
async fn foreign_key_corruption_is_detected_at_open() {
    let f = Fixture::new().await;
    let raw = f.raw().await;
    sqlx::query("PRAGMA foreign_keys=OFF")
        .execute(&raw)
        .await
        .unwrap();
    sqlx::query("INSERT INTO task_dependencies VALUES ('missing','plan',1,'a','b')")
        .execute(&raw)
        .await
        .unwrap();
    raw.close().await;
    f.store.close().await;
    assert_eq!(
        PlanStore::open(&f.path).await.unwrap_err(),
        StoreError::Integrity
    );
}
#[tokio::test]
async fn private_paths_and_snapshot_non_overwrite_are_enforced() {
    let f = Fixture::new().await;
    assert_eq!(
        f.store.snapshot(&f.path).await.unwrap_err(),
        StoreError::SnapshotExists
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::{PermissionsExt, symlink};
        assert_eq!(
            std::fs::metadata(&f.path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        let link = f.directory.path().join("linked.sqlite");
        symlink(&f.path, &link).unwrap();
        assert_eq!(
            PlanStore::open(link).await.unwrap_err(),
            StoreError::InvalidPath
        );
        f.store.close().await;
        std::fs::set_permissions(&f.path, std::fs::Permissions::from_mode(0o644)).unwrap();
        assert_eq!(
            PlanStore::open(&f.path).await.unwrap_err(),
            StoreError::InvalidPath
        );
    }
}

#[tokio::test]
async fn real_v1_upgrade_preserves_projects_and_failed_v2_leaves_v1_intact() {
    use sqlx::sqlite::SqliteConnectOptions;
    for inject_failure in [false, true] {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("upgrade.sqlite");
        let mut options = std::fs::OpenOptions::new();
        options.create_new(true).write(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        options.open(&path).unwrap();
        let raw = sqlx::SqlitePool::connect_with(
            SqliteConnectOptions::new()
                .filename(&path)
                .foreign_keys(true),
        )
        .await
        .unwrap();
        let scripts = tempfile::tempdir().unwrap();
        std::fs::write(
            scripts.path().join("0001_plan_store.sql"),
            include_str!("../migrations/0001_plan_store.sql"),
        )
        .unwrap();
        Migrator::new(scripts.path())
            .await
            .unwrap()
            .run(&raw)
            .await
            .unwrap();
        sqlx::query("INSERT INTO projects(project_id,owner_actor) VALUES ('legacy','owner')")
            .execute(&raw)
            .await
            .unwrap();
        if inject_failure {
            std::fs::write(
                scripts.path().join("0002_financial_ledger.sql"),
                format!(
                    "{}\nSELECT absent_column FROM absent_table;",
                    include_str!("../migrations/0002_financial_ledger.sql")
                ),
            )
            .unwrap();
            assert!(
                Migrator::new(scripts.path())
                    .await
                    .unwrap()
                    .run(&raw)
                    .await
                    .is_err()
            );
            assert_eq!(
                sqlx::query_scalar::<_, i64>(
                    "SELECT count(*) FROM sqlite_schema WHERE name='financial_events'"
                )
                .fetch_one(&raw)
                .await
                .unwrap(),
                0
            );
            assert_eq!(
                sqlx::query_scalar::<_, i64>("SELECT count(*) FROM _sqlx_migrations")
                    .fetch_one(&raw)
                    .await
                    .unwrap(),
                1
            );
        }
        raw.close().await;
        let upgraded = PlanStore::open(&path).await.unwrap();
        upgraded.check_integrity().await.unwrap();
        upgraded.close().await;
        let raw = sqlx::SqlitePool::connect_with(
            SqliteConnectOptions::new()
                .filename(&path)
                .foreign_keys(true),
        )
        .await
        .unwrap();
        assert_eq!(
            sqlx::query_scalar::<_, String>(
                "SELECT owner_actor FROM projects WHERE project_id='legacy'"
            )
            .fetch_one(&raw)
            .await
            .unwrap(),
            "owner"
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT count(*) FROM _sqlx_migrations WHERE success=1")
                .fetch_one(&raw)
                .await
                .unwrap(),
            16
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT count(*) FROM financial_events")
                .fetch_one(&raw)
                .await
                .unwrap(),
            0
        );
    }
}

#[tokio::test]
async fn v2_financial_data_survives_v3_and_failed_v3_rolls_back_before_repair() {
    use rolvya_persistence_sqlite::finance::*;
    use sqlx::sqlite::SqliteConnectOptions;
    for failing in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("v2.sqlite");
        let mut file = std::fs::OpenOptions::new();
        file.create_new(true).write(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            file.mode(0o600);
        }
        file.open(&db).unwrap();
        let raw = sqlx::SqlitePool::connect_with(
            SqliteConnectOptions::new().filename(&db).foreign_keys(true),
        )
        .await
        .unwrap();
        let scripts = tempfile::tempdir().unwrap();
        for (name, sql) in [
            (
                "0001_plan_store.sql",
                include_str!("../migrations/0001_plan_store.sql"),
            ),
            (
                "0002_financial_ledger.sql",
                include_str!("../migrations/0002_financial_ledger.sql"),
            ),
        ] {
            std::fs::write(scripts.path().join(name), sql).unwrap();
        }
        Migrator::new(scripts.path())
            .await
            .unwrap()
            .run(&raw)
            .await
            .unwrap();
        sqlx::query("INSERT INTO projects(project_id,owner_actor) VALUES ('legacy','owner')")
            .execute(&raw)
            .await
            .unwrap();
        sqlx::query("INSERT INTO financial_accounts VALUES ('legacy','USD')")
            .execute(&raw)
            .await
            .unwrap();
        let event:i64=sqlx::query_scalar("INSERT INTO financial_events(project_id,request_id,actor,recorded_at,kind,payload) VALUES ('legacy','approval','owner',1,'approval','{}') RETURNING sequence").fetch_one(&raw).await.unwrap();
        sqlx::query("INSERT INTO budget_approvals VALUES (?,'legacy',1,0,100,'legacy-proof',1)")
            .bind(event)
            .execute(&raw)
            .await
            .unwrap();
        let event:i64=sqlx::query_scalar("INSERT INTO financial_events(project_id,request_id,actor,recorded_at,kind,payload) VALUES ('legacy','reserve','owner',1,'reserve','{}') RETURNING sequence").fetch_one(&raw).await.unwrap();
        sqlx::query("INSERT INTO liability_revisions VALUES (?,'legacy','hold',1,0,0)")
            .bind(event)
            .execute(&raw)
            .await
            .unwrap();
        for category in ["ai_api", "compute", "storage", "transfer"] {
            sqlx::query("INSERT INTO financial_positions VALUES (?,?,0,10,0)")
                .bind(event)
                .bind(category)
                .execute(&raw)
                .await
                .unwrap();
        }
        if failing {
            std::fs::write(
                scripts.path().join("0003_scheduler_admission.sql"),
                format!(
                    "{}\nSELECT missing FROM absent;",
                    include_str!("../migrations/0003_scheduler_admission.sql")
                ),
            )
            .unwrap();
            assert!(
                Migrator::new(scripts.path())
                    .await
                    .unwrap()
                    .run(&raw)
                    .await
                    .is_err()
            );
            assert_eq!(
                sqlx::query_scalar::<_, i64>(
                    "SELECT count(*) FROM sqlite_schema WHERE name='task_attempts'"
                )
                .fetch_one(&raw)
                .await
                .unwrap(),
                0
            );
            assert_eq!(
                sqlx::query_scalar::<_, i64>("SELECT count(*) FROM _sqlx_migrations")
                    .fetch_one(&raw)
                    .await
                    .unwrap(),
                2
            );
        }
        raw.close().await;
        let store = PlanStore::open(&db).await.unwrap();
        let project = rolvya_domain::ids::ProjectId::new("legacy").unwrap();
        let balance = store.financial_snapshot(&project).await.unwrap();
        assert_eq!(balance.ceiling.minor_units(), 100);
        assert_eq!(balance.held.total().unwrap().minor_units(), 40);
        assert_eq!(balance.headroom.minor_units(), 60);
        assert_eq!(balance.version, 1);
        assert_eq!(
            store
                .financial_liability(&project, &LiabilityId::new("hold").unwrap())
                .await
                .unwrap()
                .revision,
            1
        );
        store.check_integrity().await.unwrap();
    }
}

#[tokio::test]
async fn v8_human_records_survive_v9_and_failed_v9_restores_old_guards() {
    use sqlx::sqlite::SqliteConnectOptions;
    for fail_first in [false, true] {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("upgrade.sqlite");
        let mut options = std::fs::OpenOptions::new();
        options.create_new(true).write(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        options.open(&path).unwrap();
        let raw = sqlx::SqlitePool::connect_with(
            SqliteConnectOptions::new()
                .filename(&path)
                .foreign_keys(true),
        )
        .await
        .unwrap();
        let scripts = tempfile::tempdir().unwrap();
        for entry in std::fs::read_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/migrations")).unwrap()
        {
            let entry = entry.unwrap();
            let name = entry.file_name();
            if name.to_str().unwrap() < "0009" {
                std::fs::copy(entry.path(), scripts.path().join(name)).unwrap();
            }
        }
        Migrator::new(scripts.path())
            .await
            .unwrap()
            .run(&raw)
            .await
            .unwrap();
        let user = "a".repeat(32);
        sqlx::query("INSERT INTO human_users VALUES (?,'active',1,0,10000)")
            .bind(&user)
            .execute(&raw)
            .await
            .unwrap();
        sqlx::query("INSERT INTO human_emails VALUES (?,'legacy@example.invalid',1,1,1)")
            .bind(&user)
            .execute(&raw)
            .await
            .unwrap();
        sqlx::query("INSERT INTO human_login_policy VALUES (1,1,1,1)")
            .execute(&raw)
            .await
            .unwrap();
        // Inert schema fixture, deliberately not an authenticated primary proof.
        sqlx::query("INSERT INTO human_authentications VALUES ('legacy-proof',?,'magic_link',1,1,1,10000,0,'legacy-challenge',zeroblob(32))")
            .bind(&user).execute(&raw).await.unwrap();
        let migration = include_str!("../migrations/0009_human_sessions_totp.sql");
        let next = scripts.path().join("0009_human_sessions_totp.sql");
        if fail_first {
            std::fs::write(
                &next,
                format!("{migration}\nSELECT absent_column FROM absent_table;"),
            )
            .unwrap();
            assert!(
                Migrator::new(scripts.path())
                    .await
                    .unwrap()
                    .run(&raw)
                    .await
                    .is_err()
            );
            assert_eq!(
                sqlx::query_scalar::<_, i64>("SELECT count(*) FROM _sqlx_migrations")
                    .fetch_one(&raw)
                    .await
                    .unwrap(),
                8
            );
            assert_eq!(
                sqlx::query_scalar::<_, i64>(
                    "SELECT count(*) FROM sqlite_schema WHERE name='human_sessions'"
                )
                .fetch_one(&raw)
                .await
                .unwrap(),
                0
            );
            // The replaced v8 trigger must also have rolled back.
            let trigger: String =
                sqlx::query_scalar("SELECT sql FROM sqlite_schema WHERE name='human_user_update'")
                    .fetch_one(&raw)
                    .await
                    .unwrap();
            assert!(!trigger.contains("human_factors"));
        }
        std::fs::write(&next, migration).unwrap();
        Migrator::new(scripts.path())
            .await
            .unwrap()
            .run(&raw)
            .await
            .unwrap();
        assert_eq!(
            sqlx::query_scalar::<_, String>("SELECT user_id FROM human_users")
                .fetch_one(&raw)
                .await
                .unwrap(),
            user
        );
        assert_eq!(
            sqlx::query_scalar::<_, String>("SELECT address FROM human_emails")
                .fetch_one(&raw)
                .await
                .unwrap(),
            "legacy@example.invalid"
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT count(*) FROM human_authentications")
                .fetch_one(&raw)
                .await
                .unwrap(),
            1
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT count(*) FROM human_primary_context")
                .fetch_one(&raw)
                .await
                .unwrap(),
            0
        );
        assert!(
            sqlx::query("UPDATE human_users SET factor_required=1,security_epoch=2")
                .execute(&raw)
                .await
                .is_err()
        );
        assert!(
            sqlx::query("DELETE FROM human_authentications")
                .execute(&raw)
                .await
                .is_err()
        );
        assert!(
            sqlx::query("PRAGMA foreign_key_check")
                .fetch_all(&raw)
                .await
                .unwrap()
                .is_empty()
        );
        raw.close().await;
        let upgraded = PlanStore::open(&path).await.unwrap();
        upgraded.check_integrity().await.unwrap();
    }
}

#[tokio::test]
async fn v9_humans_sessions_and_accounting_upgrade_without_implicit_owner_binding() {
    use sqlx::sqlite::SqliteConnectOptions;
    for fail_first in [false, true] {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("upgrade.sqlite");
        let mut options = std::fs::OpenOptions::new();
        options.create_new(true).write(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        options.open(&path).unwrap();
        let raw = sqlx::SqlitePool::connect_with(
            SqliteConnectOptions::new()
                .filename(&path)
                .foreign_keys(true),
        )
        .await
        .unwrap();
        let scripts = tempfile::tempdir().unwrap();
        for e in std::fs::read_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/migrations")).unwrap() {
            let e = e.unwrap();
            if e.file_name().to_str().unwrap() < "0010" {
                std::fs::copy(e.path(), scripts.path().join(e.file_name())).unwrap();
            }
        }
        Migrator::new(scripts.path())
            .await
            .unwrap()
            .run(&raw)
            .await
            .unwrap();
        // Inert historical schema fixtures: no authenticated human proof or session.
        let user = "a".repeat(32);
        let session = "b".repeat(32);
        let project = ProjectId::new("legacy-project").unwrap();
        sqlx::query("INSERT INTO projects(project_id,owner_actor) VALUES(?,?)")
            .bind(project.as_str())
            .bind(&user)
            .execute(&raw)
            .await
            .unwrap();
        sqlx::query("INSERT INTO human_users VALUES(?,'active',1,0,10000)")
            .bind(&user)
            .execute(&raw)
            .await
            .unwrap();
        sqlx::query("INSERT INTO human_emails VALUES(?,'legacy@example.invalid',1,1,1)")
            .bind(&user)
            .execute(&raw)
            .await
            .unwrap();
        sqlx::query("INSERT INTO human_login_policy VALUES(1,1,1,1)")
            .execute(&raw)
            .await
            .unwrap();
        sqlx::query("INSERT INTO human_authentications VALUES('legacy-primary',?,'magic_link',1,1,1,10000,0,'legacy-email',zeroblob(32))").bind(&user).execute(&raw).await.unwrap();
        sqlx::query("INSERT INTO human_sessions VALUES(?,?,'legacy-primary',1,1,1,1,'magic_link','primary',zeroblob(32),'synthetic-v9',10000,7786000,10000,10000,1,NULL,zeroblob(32))").bind(&session).bind(&user).execute(&raw).await.unwrap();
        sqlx::query("INSERT INTO financial_accounts VALUES(?,'USD')")
            .bind(project.as_str())
            .execute(&raw)
            .await
            .unwrap();
        let request = FinancialRequest {
            project: project.clone(),
            request_id: RequestId::new("legacy-budget").unwrap(),
            actor: PrincipalId::new(user.clone()).unwrap(),
            reason: "Inert historical accounting fixture".to_owned(),
            command: FinancialCommand::Approve {
                expected_version: 0,
                new_ceiling: rolvya_domain::money::Money::new(
                    100,
                    rolvya_domain::money::Currency::new("USD").unwrap(),
                )
                .unwrap(),
                proof: OwnerProof {
                    reference: EvidenceRef::new("legacy-budget-evidence").unwrap(),
                    verified_at: 10000,
                },
            },
        };
        let payload =
            serde_json::to_string(&serde_json::json!({"format":1,"request":request})).unwrap();
        let event:i64=sqlx::query_scalar("INSERT INTO financial_events(project_id,request_id,actor,recorded_at,kind,payload) VALUES(?,'legacy-budget',?,10000,'approval',?) RETURNING sequence").bind(project.as_str()).bind(&user).bind(&payload).fetch_one(&raw).await.unwrap();
        sqlx::query(
            "INSERT INTO budget_approvals VALUES(?,?,1,0,100,'legacy-budget-evidence',10000)",
        )
        .bind(event)
        .bind(project.as_str())
        .execute(&raw)
        .await
        .unwrap();
        let migration = include_str!("../migrations/0010_human_owner_authorization.sql");
        let next = scripts.path().join("0010_human_owner_authorization.sql");
        if fail_first {
            std::fs::write(
                &next,
                format!("{migration}\nSELECT absent_column FROM absent_table;"),
            )
            .unwrap();
            assert!(
                Migrator::new(scripts.path())
                    .await
                    .unwrap()
                    .run(&raw)
                    .await
                    .is_err()
            );
            assert_eq!(
                sqlx::query_scalar::<_, i64>("SELECT count(*) FROM _sqlx_migrations")
                    .fetch_one(&raw)
                    .await
                    .unwrap(),
                9
            );
            assert_eq!(
                sqlx::query_scalar::<_, i64>(
                    "SELECT count(*) FROM sqlite_schema WHERE name='human_memberships'"
                )
                .fetch_one(&raw)
                .await
                .unwrap(),
                0
            );
            assert_eq!(
                sqlx::query_scalar::<_, String>("SELECT payload FROM financial_events")
                    .fetch_one(&raw)
                    .await
                    .unwrap(),
                payload
            );
        }
        std::fs::write(&next, migration).unwrap();
        Migrator::new(scripts.path())
            .await
            .unwrap()
            .run(&raw)
            .await
            .unwrap();
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT count(*) FROM human_memberships")
                .fetch_one(&raw)
                .await
                .unwrap(),
            0
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT count(*) FROM human_project_bindings")
                .fetch_one(&raw)
                .await
                .unwrap(),
            0
        );
        assert_eq!(
            sqlx::query_scalar::<_, String>("SELECT owner_actor FROM projects")
                .fetch_one(&raw)
                .await
                .unwrap(),
            user
        );
        assert_eq!(
            sqlx::query_scalar::<_, String>("SELECT session_id FROM human_sessions")
                .fetch_one(&raw)
                .await
                .unwrap(),
            session
        );
        assert_eq!(
            sqlx::query_scalar::<_, String>("SELECT payload FROM financial_events")
                .fetch_one(&raw)
                .await
                .unwrap(),
            payload
        );
        assert!(
            sqlx::query("DELETE FROM financial_events")
                .execute(&raw)
                .await
                .is_err()
        );
        assert!(
            sqlx::query("UPDATE human_sessions SET generation=0")
                .execute(&raw)
                .await
                .is_err()
        );
        raw.close().await;
        let upgraded = PlanStore::open(&path).await.unwrap();
        upgraded.check_integrity().await.unwrap();
        assert_eq!(
            upgraded
                .financial_snapshot(&project)
                .await
                .unwrap()
                .ceiling
                .minor_units(),
            100
        );
    }
}

#[tokio::test]
async fn populated_v11_upgrade_and_v12_failure_preserve_all_prior_authority_without_accounts() {
    use sqlx::sqlite::SqliteConnectOptions;
    for fail in [false, true] {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("v11.sqlite");
        let mut options = std::fs::OpenOptions::new();
        options.create_new(true).write(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        options.open(&path).unwrap();
        let raw = sqlx::SqlitePool::connect_with(
            SqliteConnectOptions::new()
                .filename(&path)
                .foreign_keys(true),
        )
        .await
        .unwrap();
        let scripts = tempfile::tempdir().unwrap();
        for e in std::fs::read_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/migrations")).unwrap() {
            let e = e.unwrap();
            if e.file_name().to_str().unwrap() < "0012" {
                std::fs::copy(e.path(), scripts.path().join(e.file_name())).unwrap();
            }
        }
        Migrator::new(scripts.path())
            .await
            .unwrap()
            .run(&raw)
            .await
            .unwrap();
        sqlx::query("INSERT INTO projects(project_id,owner_actor) VALUES('retained-project','legacy-owner')").execute(&raw).await.unwrap();
        let before: Vec<(String, String)> =
            sqlx::query_as("SELECT project_id,owner_actor FROM projects")
                .fetch_all(&raw)
                .await
                .unwrap();
        let next = scripts
            .path()
            .join("0012_provider_account_entitlements.sql");
        let migration = include_str!("../migrations/0012_provider_account_entitlements.sql");
        if fail {
            std::fs::write(
                &next,
                format!("{migration}\nSELECT missing_column FROM missing_table;"),
            )
            .unwrap();
            assert!(
                Migrator::new(scripts.path())
                    .await
                    .unwrap()
                    .run(&raw)
                    .await
                    .is_err()
            );
            assert_eq!(
                sqlx::query_scalar::<_, i64>("SELECT count(*) FROM _sqlx_migrations")
                    .fetch_one(&raw)
                    .await
                    .unwrap(),
                11
            );
            assert_eq!(
                sqlx::query_scalar::<_, i64>(
                    "SELECT count(*) FROM sqlite_schema WHERE name='provider_connections'"
                )
                .fetch_one(&raw)
                .await
                .unwrap(),
                0
            );
        }
        std::fs::write(&next, migration).unwrap();
        Migrator::new(scripts.path())
            .await
            .unwrap()
            .run(&raw)
            .await
            .unwrap();
        assert_eq!(
            sqlx::query_as::<_, (String, String)>("SELECT project_id,owner_actor FROM projects")
                .fetch_all(&raw)
                .await
                .unwrap(),
            before
        );
        for table in [
            "provider_connections",
            "provider_capacity_reservations",
            "provider_project_grants",
        ] {
            assert_eq!(
                sqlx::query_scalar::<_, i64>(&format!("SELECT count(*) FROM {table}"))
                    .fetch_one(&raw)
                    .await
                    .unwrap(),
                0
            );
        }
        raw.close().await;
        let store = PlanStore::open(&path).await.unwrap();
        store.check_integrity().await.unwrap();
    }
}

#[tokio::test]
async fn populated_v12_upgrade_and_v13_failure_preserve_prior_authority_without_cloud_grants() {
    use sqlx::sqlite::SqliteConnectOptions;
    for fail in [false, true] {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("v12.sqlite");
        let mut options = std::fs::OpenOptions::new();
        options.create_new(true).write(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        options.open(&path).unwrap();
        let raw = sqlx::SqlitePool::connect_with(
            SqliteConnectOptions::new()
                .filename(&path)
                .foreign_keys(true),
        )
        .await
        .unwrap();
        let scripts = tempfile::tempdir().unwrap();
        for e in std::fs::read_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/migrations")).unwrap() {
            let e = e.unwrap();
            if e.file_name().to_str().unwrap() < "0013" {
                std::fs::copy(e.path(), scripts.path().join(e.file_name())).unwrap();
            }
        }
        Migrator::new(scripts.path())
            .await
            .unwrap()
            .run(&raw)
            .await
            .unwrap();
        sqlx::query("INSERT INTO projects(project_id,owner_actor) VALUES('retained-project','legacy-owner')").execute(&raw).await.unwrap();
        let before: Vec<(String, String)> =
            sqlx::query_as("SELECT project_id,owner_actor FROM projects")
                .fetch_all(&raw)
                .await
                .unwrap();
        let next = scripts.path().join("0013_aws_fargate_identity.sql");
        let migration = include_str!("../migrations/0013_aws_fargate_identity.sql");
        if fail {
            std::fs::write(
                &next,
                format!("{migration}\nSELECT missing_column FROM missing_table;"),
            )
            .unwrap();
            assert!(
                Migrator::new(scripts.path())
                    .await
                    .unwrap()
                    .run(&raw)
                    .await
                    .is_err()
            );
            assert_eq!(
                sqlx::query_scalar::<_, i64>("SELECT count(*) FROM _sqlx_migrations")
                    .fetch_one(&raw)
                    .await
                    .unwrap(),
                12
            );
            assert_eq!(
                sqlx::query_scalar::<_, i64>(
                    "SELECT count(*) FROM sqlite_schema WHERE name='aws_infrastructure_accounts'"
                )
                .fetch_one(&raw)
                .await
                .unwrap(),
                0
            );
        }
        std::fs::write(&next, migration).unwrap();
        Migrator::new(scripts.path())
            .await
            .unwrap()
            .run(&raw)
            .await
            .unwrap();
        assert_eq!(
            sqlx::query_as::<_, (String, String)>("SELECT project_id,owner_actor FROM projects")
                .fetch_all(&raw)
                .await
                .unwrap(),
            before
        );
        for table in [
            "aws_infrastructure_accounts",
            "aws_execution_bindings",
            "aws_task_inspections",
        ] {
            assert_eq!(
                sqlx::query_scalar::<_, i64>(&format!("SELECT count(*) FROM {table}"))
                    .fetch_one(&raw)
                    .await
                    .unwrap(),
                0
            );
        }
        raw.close().await;
        let store = PlanStore::open(&path).await.unwrap();
        store.check_integrity().await.unwrap();
    }
}

#[tokio::test]
async fn populated_v13_upgrade_and_v14_failure_leave_core_authority_intact() {
    use sqlx::sqlite::SqliteConnectOptions;
    for fail in [false, true] {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("upgrade.sqlite");
        let raw = sqlx::SqlitePool::connect_with(
            SqliteConnectOptions::new()
                .filename(&path)
                .create_if_missing(true)
                .foreign_keys(true),
        )
        .await
        .unwrap();
        let scripts = tempfile::tempdir().unwrap();
        for entry in std::fs::read_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/migrations")).unwrap()
        {
            let e = entry.unwrap();
            if e.file_name().to_str().unwrap() < "0014" {
                std::fs::copy(e.path(), scripts.path().join(e.file_name())).unwrap();
            }
        }
        Migrator::new(scripts.path())
            .await
            .unwrap()
            .run(&raw)
            .await
            .unwrap();
        sqlx::query("INSERT INTO projects(project_id,owner_actor) VALUES('retained-project','legacy-owner')").execute(&raw).await.unwrap();
        let migration = include_str!("../migrations/0014_aws_protected_bootstrap.sql");
        let next = scripts.path().join("0014_aws_protected_bootstrap.sql");
        if fail {
            std::fs::write(
                &next,
                format!("{migration}\nSELECT absent FROM missing_table;"),
            )
            .unwrap();
            assert!(
                Migrator::new(scripts.path())
                    .await
                    .unwrap()
                    .run(&raw)
                    .await
                    .is_err()
            );
            assert_eq!(
                sqlx::query_scalar::<_, i64>("SELECT count(*) FROM _sqlx_migrations")
                    .fetch_one(&raw)
                    .await
                    .unwrap(),
                13
            );
            assert_eq!(
                sqlx::query_scalar::<_, i64>(
                    "SELECT count(*) FROM sqlite_schema WHERE name='aws_protected_challenges'"
                )
                .fetch_one(&raw)
                .await
                .unwrap(),
                0
            );
        }
        std::fs::write(&next, migration).unwrap();
        Migrator::new(scripts.path())
            .await
            .unwrap()
            .run(&raw)
            .await
            .unwrap();
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT count(*) FROM _sqlx_migrations")
                .fetch_one(&raw)
                .await
                .unwrap(),
            14
        );
        assert_eq!(
            sqlx::query_scalar::<_, String>(
                "SELECT owner_actor FROM projects WHERE project_id='retained-project'"
            )
            .fetch_one(&raw)
            .await
            .unwrap(),
            "legacy-owner"
        );
        assert_eq!(
            sqlx::query_scalar::<_, String>("PRAGMA integrity_check")
                .fetch_one(&raw)
                .await
                .unwrap(),
            "ok"
        );
        assert!(
            sqlx::query("PRAGMA foreign_key_check")
                .fetch_all(&raw)
                .await
                .unwrap()
                .is_empty()
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT count(*) FROM aws_protected_challenges")
                .fetch_one(&raw)
                .await
                .unwrap(),
            0
        );
    }
}
#[tokio::test]
async fn populated_v14_upgrade_and_v15_failure_preserve_prior_data() {
    use sqlx::sqlite::SqliteConnectOptions;
    for fail in [false, true] {
        let root = tempfile::tempdir().unwrap();
        // Keep the populated-upgrade fixture private from creation, regardless
        // of the runner umask; PlanStore correctly rejects a broad raw DB file.
        let private = tempfile::NamedTempFile::new_in(root.path()).unwrap();
        let path = private.path().to_path_buf();
        let raw = sqlx::SqlitePool::connect_with(
            SqliteConnectOptions::new()
                .filename(&path)
                .create_if_missing(true)
                .foreign_keys(true),
        )
        .await
        .unwrap();
        let scripts = tempfile::tempdir().unwrap();
        for e in std::fs::read_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/migrations")).unwrap() {
            let e = e.unwrap();
            if e.file_name().to_str().unwrap() < "0015" {
                std::fs::copy(e.path(), scripts.path().join(e.file_name())).unwrap();
            }
        }
        Migrator::new(scripts.path())
            .await
            .unwrap()
            .run(&raw)
            .await
            .unwrap();
        sqlx::query("INSERT INTO projects(project_id,owner_actor) VALUES('retained','legacy-never-a-human-owner')").execute(&raw).await.unwrap();
        let next = scripts.path().join("0015_aws_execution_lifecycle.sql");
        let sql = include_str!("../migrations/0015_aws_execution_lifecycle.sql");
        if fail {
            std::fs::write(
                &next,
                format!("{sql}\nSELECT nonexistent FROM missing_table;"),
            )
            .unwrap();
            assert!(
                Migrator::new(scripts.path())
                    .await
                    .unwrap()
                    .run(&raw)
                    .await
                    .is_err()
            );
            assert_eq!(
                sqlx::query_scalar::<_, i64>("SELECT count(*) FROM _sqlx_migrations")
                    .fetch_one(&raw)
                    .await
                    .unwrap(),
                14
            );
            assert_eq!(
                sqlx::query_scalar::<_, i64>(
                    "SELECT count(*) FROM sqlite_schema WHERE name='aws_launch_requests'"
                )
                .fetch_one(&raw)
                .await
                .unwrap(),
                0
            );
        }
        std::fs::write(&next, sql).unwrap();
        Migrator::new(scripts.path())
            .await
            .unwrap()
            .run(&raw)
            .await
            .unwrap();
        assert_eq!(
            sqlx::query_scalar::<_, String>(
                "SELECT owner_actor FROM projects WHERE project_id='retained'"
            )
            .fetch_one(&raw)
            .await
            .unwrap(),
            "legacy-never-a-human-owner"
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT count(*) FROM _sqlx_migrations")
                .fetch_one(&raw)
                .await
                .unwrap(),
            15
        );
        assert!(
            sqlx::query("PRAGMA foreign_key_check")
                .fetch_all(&raw)
                .await
                .unwrap()
                .is_empty()
        );
        assert_eq!(
            sqlx::query_scalar::<_, String>("PRAGMA integrity_check")
                .fetch_one(&raw)
                .await
                .unwrap(),
            "ok"
        );
        raw.close().await;
        PlanStore::open(path.as_path())
            .await
            .unwrap()
            .check_integrity()
            .await
            .unwrap();
    }
}

#[tokio::test]
async fn v15_data_survives_v16_and_failed_git_migration_rolls_back() {
    for fail in [false, true] {
        let scripts = tempfile::tempdir().unwrap();
        for e in std::fs::read_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/migrations")).unwrap() {
            let e = e.unwrap();
            if e.file_name().to_str().unwrap() < "0016" {
                std::fs::copy(e.path(), scripts.path().join(e.file_name())).unwrap();
            }
        }
        let path = tempfile::NamedTempFile::new().unwrap();
        let raw = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(
                sqlx::sqlite::SqliteConnectOptions::new()
                    .filename(path.path())
                    .foreign_keys(true),
            )
            .await
            .unwrap();
        Migrator::new(scripts.path())
            .await
            .unwrap()
            .run(&raw)
            .await
            .unwrap();
        sqlx::query("INSERT INTO projects(project_id,owner_actor) VALUES('git-upgrade','legacy-not-a-human')").execute(&raw).await.unwrap();
        let sql = include_str!("../migrations/0016_github_trusted_publication.sql");
        let next = scripts.path().join("0016_github_trusted_publication.sql");
        if fail {
            std::fs::write(&next, format!("{sql}\nSELECT bad FROM absent_table;")).unwrap();
            assert!(
                Migrator::new(scripts.path())
                    .await
                    .unwrap()
                    .run(&raw)
                    .await
                    .is_err()
            );
            assert_eq!(
                sqlx::query_scalar::<_, i64>("SELECT count(*) FROM _sqlx_migrations")
                    .fetch_one(&raw)
                    .await
                    .unwrap(),
                15
            );
            assert_eq!(
                sqlx::query_scalar::<_, i64>(
                    "SELECT count(*) FROM sqlite_schema WHERE name='git_bindings'"
                )
                .fetch_one(&raw)
                .await
                .unwrap(),
                0
            );
        }
        std::fs::write(next, sql).unwrap();
        Migrator::new(scripts.path())
            .await
            .unwrap()
            .run(&raw)
            .await
            .unwrap();
        assert_eq!(
            sqlx::query_scalar::<_, String>(
                "SELECT owner_actor FROM projects WHERE project_id='git-upgrade'"
            )
            .fetch_one(&raw)
            .await
            .unwrap(),
            "legacy-not-a-human"
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT count(*) FROM _sqlx_migrations")
                .fetch_one(&raw)
                .await
                .unwrap(),
            16
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT count(*) FROM git_bindings")
                .fetch_one(&raw)
                .await
                .unwrap(),
            0
        );
        raw.close().await;
        PlanStore::open(path.path())
            .await
            .unwrap()
            .check_integrity()
            .await
            .unwrap();
    }
}
