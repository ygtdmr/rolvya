//! Narrow historical evidence reader. It cannot create or consume an acceptance.
use super::*;
use crate::controller_keys::ControllerKey;
use subtle::ConstantTimeEq;
pub(crate) struct PanelArtifactEvidence {
    pub digest: String,
    pub acceptance: Option<String>,
    pub tests: Vec<String>,
    pub verification: &'static str,
}
pub(crate) async fn panel_artifact_in(
    c: &mut SqliteConnection,
    key: &ControllerKey,
    l: &LeaseRef,
    fingerprint: &str,
) -> Result<Option<PanelArtifactEvidence>, OrchestrationError> {
    let coded: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM coding_artifacts WHERE project_id=? AND attempt_id=?)",
    )
    .bind(l.task.plan.project_id.as_str())
    .bind(l.attempt_id.as_str())
    .fetch_one(&mut *c)
    .await?;
    let query = if coded {
        "SELECT content,sha256 FROM coding_artifacts WHERE project_id=? AND attempt_id=?"
    } else {
        "SELECT content,sha256 FROM synthetic_artifacts WHERE project_id=? AND attempt_id=?"
    };
    let Some(r) = sqlx::query(query)
        .bind(l.task.plan.project_id.as_str())
        .bind(l.attempt_id.as_str())
        .fetch_optional(&mut *c)
        .await?
    else {
        return Ok(None);
    };
    let raw: Vec<u8> = r.try_get("content")?;
    let digest: String = r.try_get("sha256")?;
    if raw.is_empty() || raw.len() > 65536 || hash(&raw) != digest {
        return Err(OrchestrationError::InvalidArtifact);
    }
    let r=sqlx::query("SELECT a.manifest_sha256,v.manifest FROM task_acceptances a JOIN synthetic_validations v ON v.project_id=a.project_id AND v.plan_id=a.plan_id AND v.version=a.version AND v.task_id=a.task_id AND v.sha256=a.manifest_sha256 JOIN task_runtime t USING(project_id,plan_id,version,task_id) WHERE a.project_id=? AND a.plan_id=? AND a.version=? AND a.task_id=? AND a.attempt_id=? AND a.epoch=? AND a.fingerprint=? AND t.state='succeeded'")
        .bind(l.task.plan.project_id.as_str()).bind(l.task.plan.plan_id.as_str()).bind(version(&l.task.plan)?).bind(l.task.task_id.as_str()).bind(l.attempt_id.as_str()).bind(l.epoch as i64).bind(fingerprint).fetch_optional(&mut *c).await?;
    let Some(r) = r else {
        let report = if coded {
            panel_report(c, key, l, &digest).await?
        } else {
            None
        };
        let rejected: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM coding_rejections WHERE project_id=? AND attempt_id=?)",
        )
        .bind(l.task.plan.project_id.as_str())
        .bind(l.attempt_id.as_str())
        .fetch_one(&mut *c)
        .await?;
        return Ok(Some(PanelArtifactEvidence {
            digest,
            acceptance: None,
            verification: if report.is_some() {
                "passed"
            } else if rejected {
                "failed"
            } else {
                "pending"
            },
            tests: report.map(|r| r.tests_completed).unwrap_or_default(),
        }));
    };
    let bytes: Vec<u8> = r.try_get("manifest")?;
    let accepted: String = r.try_get("manifest_sha256")?;
    if bytes.len() > 65536 || hash(&bytes) != accepted {
        return Err(OrchestrationError::InvalidArtifact);
    }
    let m: artifacts::Manifest =
        serde_json::from_slice(&bytes).map_err(|_| OrchestrationError::InvalidArtifact)?;
    if m.task != l.task
        || m.lease.as_ref() != Some(l)
        || m.plan_hash.as_str() != fingerprint
        || m.artifact.as_ref().map(Digest::as_str) != Some(digest.as_str())
    {
        return Err(OrchestrationError::InvalidArtifact);
    }
    let tests = if coded {
        if m.format != 2 || m.coding.is_none() {
            return Err(OrchestrationError::InvalidArtifact);
        }
        let row=sqlx::query("SELECT d.manifest_sha256,d.seal AS decision,v.artifact_sha256,v.evidence,v.seal AS proof FROM coding_decisions d JOIN coding_verifications v USING(project_id,attempt_id) WHERE d.project_id=? AND d.attempt_id=?")
            .bind(l.task.plan.project_id.as_str()).bind(l.attempt_id.as_str()).fetch_optional(&mut *c).await?.ok_or(OrchestrationError::VerificationUnavailable)?;
        let report: coding::VerificationReport =
            serde_json::from_slice(&row.try_get::<Vec<u8>, _>("evidence")?)
                .map_err(|_| OrchestrationError::InvalidArtifact)?;
        let decision = key.coding_record_seal(
            b"decision",
            &serde_json::to_vec(&(l, &accepted))
                .map_err(|_| OrchestrationError::InvalidArtifact)?,
        );
        let evidence = key.coding_record_seal(
            b"evidence",
            &serde_json::to_vec(&(l, &digest, &report))
                .map_err(|_| OrchestrationError::InvalidArtifact)?,
        );
        if row.try_get::<&str, _>("manifest_sha256")? != accepted
            || row.try_get::<&str, _>("artifact_sha256")? != digest
            || !bool::from(
                decision
                    .as_slice()
                    .ct_eq(&row.try_get::<Vec<u8>, _>("decision")?),
            )
            || !bool::from(
                evidence
                    .as_slice()
                    .ct_eq(&row.try_get::<Vec<u8>, _>("proof")?),
            )
            || report.artifact_sha256.as_str() != digest
            || report.binding.task != l.task
            || report.binding.attempt_id != l.attempt_id
            || report.binding.epoch != l.epoch
            || report.terminal != coding::VerificationTerminal::Completed
            || report.exit_code != 0
            || report.tests_completed.len() > 64
            || report
                .tests_completed
                .iter()
                .any(|t| t.len() > 256 || t.chars().any(char::is_control))
        {
            return Err(OrchestrationError::InvalidArtifact);
        }
        report.tests_completed
    } else {
        if m.format != 1 || m.coding.is_some() {
            return Err(OrchestrationError::InvalidArtifact);
        }
        let j = artifacts::job(c, l).await?;
        let text:String=sqlx::query_scalar("SELECT definition FROM tasks WHERE project_id=? AND plan_id=? AND version=? AND task_id=?")
            .bind(l.task.plan.project_id.as_str()).bind(l.task.plan.plan_id.as_str()).bind(version(&l.task.plan)?).bind(l.task.task_id.as_str()).fetch_one(&mut *c).await?;
        let task: PlannedTask =
            serde_json::from_str(&text).map_err(|_| OrchestrationError::InvalidArtifact)?;
        artifacts::check_artifact(&task, &j, &raw, &Digest::new(digest.clone())?)?;
        vec![] // Synthetic structural predicates are not executed source tests.
    };
    Ok(Some(PanelArtifactEvidence {
        digest,
        acceptance: Some(accepted),
        tests,
        verification: "passed",
    }))
}

async fn panel_report(
    c: &mut SqliteConnection,
    key: &ControllerKey,
    l: &LeaseRef,
    digest: &str,
) -> Result<Option<coding::VerificationReport>, OrchestrationError> {
    let row = sqlx::query("SELECT evidence,artifact_sha256,seal FROM coding_verifications WHERE project_id=? AND attempt_id=?").bind(l.task.plan.project_id.as_str()).bind(l.attempt_id.as_str()).fetch_optional(&mut *c).await?;
    let Some(row) = row else {
        return Ok(None);
    };
    let raw: Vec<u8> = row.try_get("evidence")?;
    if raw.len() > 32768 {
        return Err(OrchestrationError::InvalidArtifact);
    }
    let report: coding::VerificationReport =
        serde_json::from_slice(&raw).map_err(|_| OrchestrationError::InvalidArtifact)?;
    let seal = key.coding_record_seal(
        b"evidence",
        &serde_json::to_vec(&(l, digest, &report))
            .map_err(|_| OrchestrationError::InvalidArtifact)?,
    );
    if row.try_get::<&str, _>("artifact_sha256")? != digest
        || !bool::from(seal.as_slice().ct_eq(&row.try_get::<Vec<u8>, _>("seal")?))
        || report.artifact_sha256.as_str() != digest
        || report.binding.task != l.task
        || report.binding.attempt_id != l.attempt_id
        || report.binding.epoch != l.epoch
        || report.terminal != coding::VerificationTerminal::Completed
        || report.exit_code != 0
        || report.tests_completed.len() > 64
        || report
            .tests_completed
            .iter()
            .any(|t| t.len() > 256 || t.chars().any(char::is_control))
    {
        return Err(OrchestrationError::InvalidArtifact);
    }
    Ok(Some(report))
}
