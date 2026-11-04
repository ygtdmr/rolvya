use rolvya_persistence_sqlite::controller_keys::{
    human_auth::{HumanId, Password},
    operations::*,
};
use std::{
    io::{IsTerminal, Read},
    path::Path,
};
use zeroize::Zeroizing;

fn input(limit: usize) -> Result<Zeroizing<Vec<u8>>, OperationError> {
    if std::io::stdin().is_terminal() {
        return Err(OperationError::Unauthorized);
    }
    let mut bytes = Zeroizing::new(Vec::new());
    std::io::stdin()
        .lock()
        .take(limit as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| OperationError::Storage)?;
    if bytes.len() > limit {
        return Err(OperationError::ResourceLimit);
    }
    Ok(bytes)
}
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct SetupInput {
    request: String,
    sha256: String,
}
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct HumanInput {
    spec: EnrollmentSpec,
    password: String,
}
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct BindingInput {
    project: String,
    human: String,
    legacy: String,
}
const INSPECTED_STATUS: &str = r#"{"level":"configuration_inspected","execution_allowed":false}"#;
#[tokio::main]
async fn main() {
    if let Err(error) = run().await {
        eprintln!("{error}");
        std::process::exit(1)
    }
}
async fn run() -> Result<(), OperationError> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let command = args
        .first()
        .map(String::as_str)
        .ok_or(OperationError::InvalidConfiguration)?;
    if args.len() == 2 && command == "asset-digest" {
        println!("{}", asset_digest(Path::new(&args[1]))?);
        return Ok(());
    }
    if args.len() == 2 && command == "generate-secret" {
        HostOperator::generate_protection_secret(Path::new(&args[1]))?;
        println!("protection secret published to restricted file");
        return Ok(());
    }
    if !(2..=4).contains(&args.len()) {
        return Err(OperationError::InvalidConfiguration);
    }
    let config = DeploymentConfig::load(Path::new(&args[1]))?;
    if command == "migrate" && args.len() == 2 {
        HostOperator::migrate(&config).await?;
        println!("reviewed schema migration verified; execution remains disabled");
        return Ok(());
    }
    if command == "serve" && args.len() == 2 {
        let service = ControllerService::prepare(config).await?;
        let mut term = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .map_err(|_| OperationError::Unavailable)?;
        return service
            .serve(async move {
                tokio::select! {_ = term.recv()=>{},_ = tokio::signal::ctrl_c()=>{}}
            })
            .await;
    }
    if command == "restore-db" && args.len() == 4 {
        let source = FilesystemDestination::open(Path::new(&args[2]))?;
        let r = HostOperator::restore_database(&config, &source, &args[3]).await?;
        println!(
            "{}",
            serde_json::to_string(&r).map_err(|_| OperationError::Integrity)?
        );
        return Ok(());
    }
    let op = HostOperator::open(config).await?;
    let result = match (command, args.len()) {
        ("inspect", 2) => {
            println!("{}", op.state().await?);
            Ok(())
        }
        ("prepare-setup", 2) => {
            let r = op.prepare_setup().await?;
            println!(
                "{}",
                serde_json::to_string(&r).map_err(|_| OperationError::Integrity)?
            );
            Ok(())
        }
        ("initialize", 2) => {
            let body = input(4096)?;
            let v: SetupInput =
                serde_json::from_slice(&body).map_err(|_| OperationError::InvalidConfiguration)?;
            op.initialize(&v.request, &v.sha256).await
        }
        ("prepare-human", 2) => {
            let bytes = input(8192)?;
            let body: HumanInput =
                serde_json::from_slice(&bytes).map_err(|_| OperationError::InvalidConfiguration)?;
            let password = Password::new(body.password.into_bytes())
                .map_err(|_| OperationError::InvalidConfiguration)?;
            let r = op.prepare_enrollment(&body.spec, Some(&password)).await?;
            println!(
                "{}",
                serde_json::to_string(&r).map_err(|_| OperationError::Integrity)?
            );
            Ok(())
        }
        ("enroll-human", 3) => {
            let proof = SignedEnrollment::load(Path::new(&args[2]))?;
            let password = Password::new(input(1024)?.to_vec())
                .map_err(|_| OperationError::InvalidConfiguration)?;
            let human = op.enroll(&proof, Some(&password)).await?;
            println!("enrolled human {}", human.as_str());
            Ok(())
        }
        ("prepare-binding", 2) => {
            let bytes = input(4096)?;
            let body: BindingInput =
                serde_json::from_slice(&bytes).map_err(|_| OperationError::InvalidConfiguration)?;
            let project = rolvya_domain::ids::ProjectId::new(&body.project)
                .map_err(|_| OperationError::InvalidConfiguration)?;
            let human =
                HumanId::parse(body.human).map_err(|_| OperationError::InvalidConfiguration)?;
            let r = op.prepare_binding(&project, &human, &body.legacy).await?;
            println!(
                "{}",
                serde_json::to_string(&r).map_err(|_| OperationError::Integrity)?
            );
            Ok(())
        }
        ("inspect-qualification", 2) => {
            let body: QualificationInspection = serde_json::from_slice(&input(131072)?)
                .map_err(|_| OperationError::InvalidConfiguration)?;
            println!(
                "{}",
                serde_json::to_string(&op.inspect_qualification(&body).await?)
                    .map_err(|_| OperationError::Integrity)?
            );
            Ok(())
        }
        ("inspect-account-qualification", 2) => {
            let body: AccountInspection = serde_json::from_slice(&input(16384)?)
                .map_err(|_| OperationError::InvalidConfiguration)?;
            op.inspect_account_qualification(&body).await?;
            println!("{INSPECTED_STATUS}");
            Ok(())
        }
        ("qualification-report", 2) => {
            let body:rolvya_persistence_sqlite::controller_keys::qualification::QualificationScope=serde_json::from_slice(&input(8192)?).map_err(|_|OperationError::InvalidConfiguration)?;
            println!(
                "{}",
                serde_json::to_string(&op.qualification_report(&body).await?)
                    .map_err(|_| OperationError::Integrity)?
            );
            Ok(())
        }
        ("verify-qualification", 2) => {
            let body: rolvya_persistence_sqlite::controller_keys::qualification::SignedEvidence =
                serde_json::from_slice(&input(16384)?)
                    .map_err(|_| OperationError::InvalidConfiguration)?;
            op.verify_qualification(&body).await?;
            println!("qualification evidence committed; execution remains separately gated");
            Ok(())
        }
        ("revoke-qualification", 2) => {
            let body: QualificationRevocation = serde_json::from_slice(&input(4096)?)
                .map_err(|_| OperationError::InvalidConfiguration)?;
            op.revoke_qualification(&body).await?;
            println!("qualification revoked; financial holds preserved");
            Ok(())
        }
        ("estimate-exposure", 2) => {
            let body: ExposureInspection = serde_json::from_slice(&input(65536)?)
                .map_err(|_| OperationError::InvalidConfiguration)?;
            println!(
                "{}",
                serde_json::to_string(&op.estimate_exposure(&body)?)
                    .map_err(|_| OperationError::Integrity)?
            );
            Ok(())
        }
        ("inspect-provider-operations", 2) => {
            let query:rolvya_persistence_sqlite::controller_keys::worker_auth::aws_fargate::execution::ReconciliationQuery=serde_json::from_slice(&input(4096)?).map_err(|_|OperationError::InvalidConfiguration)?;
            println!(
                "{}",
                serde_json::to_string(&op.inspect_provider_operations(&query).await?)
                    .map_err(|_| OperationError::Integrity)?
            );
            Ok(())
        }
        ("readiness", 2) => {
            println!(
                "{}",
                serde_json::to_string(&op.readiness().await?)
                    .map_err(|_| OperationError::Integrity)?
            );
            Ok(())
        }
        ("backup", 3) => {
            let class = match args[2].as_str() {
                "frequent" => BackupClass::Frequent,
                "daily" => BackupClass::Daily,
                _ => return Err(OperationError::InvalidConfiguration),
            };
            println!(
                "{}",
                serde_json::to_string(&op.backup(class).await?)
                    .map_err(|_| OperationError::Integrity)?
            );
            Ok(())
        }
        ("retention", 2) => {
            println!("{}", op.retain_backups().await?);
            Ok(())
        }
        ("export-kit", 3) => {
            op.export_recovery_kit(Path::new(&args[2]), &input(32)?)
                .await
        }
        ("recover-key", 3) => {
            let receipt = op
                .recover_controller_key(Path::new(&args[2]), &input(32)?)
                .await?;
            println!(
                "{}",
                serde_json::to_string(&receipt).map_err(|_| OperationError::Integrity)?
            );
            Ok(())
        }
        _ => Err(OperationError::InvalidConfiguration),
    };
    op.close().await;
    result
}

#[cfg(test)]
mod tests {
    #[test]
    fn account_inspection_diagnostics_expose_only_static_validation_status() {
        let value: serde_json::Value = serde_json::from_str(super::INSPECTED_STATUS).unwrap();
        let fields = value.as_object().unwrap();
        assert_eq!(
            fields.len(),
            2,
            "Account inspection must not export account-derived fingerprints or credential metadata"
        );
        assert_eq!(fields["level"], "configuration_inspected");
        assert_eq!(fields["execution_allowed"], false);
    }
}
