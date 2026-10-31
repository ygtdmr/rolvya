//! Per-call evidence under existing attempt authority. Missing completion is an
//! unknown external effect, never permission to repeat a paid request.
use super::store::{decode, same};
use super::*;
use crate::controller_keys::worker_auth::https::agent::{
    Adapter, AgentOperation, ExecutionEvidence, StepReceipt,
};
#[derive(Serialize, Deserialize, Clone, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct Call {
    work: Digest,
    step: u16,
    adapter: Adapter,
    request: Digest,
    maximum: i64,
}
impl SyntheticCoordinator {
    pub(crate) async fn coding_agent_in(
        &self,
        c: &mut SqliteConnection,
        l: &LeaseRef,
        op: &AgentOperation,
        now: i64,
    ) -> Result<StepReceipt, OrchestrationError> {
        let w = self.coding_work_in(c, l, now).await?;
        let current = crate::controller_keys::provider_accounts::coding_model_binding(
            c,
            &self.coding_authority()?.key,
            &l.task.plan.project_id,
            &w.connection,
        )
        .await
        .map_err(|_| OrchestrationError::ScopeMismatch)?;
        same(
            w.model_account
                .as_ref()
                .ok_or(OrchestrationError::VerificationUnavailable)?,
            &current,
        )?;
        // No current production provider permits remote consumption with verified
        // isolation AND independently enforceable cost bounds. Private fixtures
        // exercise this implementation; no request object can enable it.
        #[cfg(not(test))]
        let terms: Option<i64> = None;
        #[cfg(test)]
        let terms = self.coding_authority()?.agent_maximum;
        let maximum = terms.ok_or(OrchestrationError::VerificationUnavailable)?;
        if !(0..=1_000_000).contains(&maximum) {
            return Err(OrchestrationError::LimitExceeded);
        }
        let (work, step) = match op {
            AgentOperation::Start { work, step, .. }
            | AgentOperation::Finish { work, step, .. } => (work, *step),
        };
        if work != &w.identity || !(1..=8).contains(&step) {
            return Err(OrchestrationError::ScopeMismatch);
        }
        let rows=sqlx::query("SELECT body,seal FROM coding_agent_calls WHERE project_id=? AND attempt_id=? ORDER BY step").bind(l.task.plan.project_id.as_str()).bind(l.attempt_id.as_str()).fetch_all(&mut *c).await?;
        let mut calls = Vec::new();
        let mut used = 0i64;
        for row in rows {
            let call: Call = decode(&row.get::<Vec<u8>, _>(0), 4096)?;
            self.coding_authority()?
                .verify(b"provider-call", &call, &row.get::<Vec<u8>, _>(1))?;
            if call.work != w.identity
                || usize::from(call.step) != calls.len() + 1
                || call.maximum < 0
            {
                return Err(OrchestrationError::InvalidArtifact);
            }
            used = used
                .checked_add(call.maximum)
                .ok_or(OrchestrationError::LimitExceeded)?;
            calls.push(call);
        }
        let mut execute = false;
        let call = match op {
            AgentOperation::Start {
                adapter,
                request_sha256,
                ..
            } => {
                crate::worker_control::coding_provider_ready(c, l, now).await?;
                if !adapter.matches_binding(
                    w.model_account
                        .as_ref()
                        .ok_or(OrchestrationError::ScopeMismatch)?,
                ) {
                    return Err(OrchestrationError::ScopeMismatch);
                }
                if (adapter == &Adapter::OpenAiApi || adapter == &Adapter::AnthropicApi)
                    && w.profile.access_mode != rolvya_domain::plan::AccessMode::ApiKey
                {
                    return Err(OrchestrationError::ScopeMismatch);
                }
                let call = Call {
                    work: w.identity.clone(),
                    step,
                    adapter: *adapter,
                    request: request_sha256.clone(),
                    maximum,
                };
                if let Some(old) = calls.get(usize::from(step) - 1) {
                    same(old, &call)?;
                } else {
                    if usize::from(step) != calls.len() + 1
                        || calls.first().is_some_and(|p| p.adapter != *adapter)
                    {
                        return Err(OrchestrationError::Conflict);
                    }
                    if let Some(previous) = calls.last() {
                        let evidence = self
                            .agent_observation(c, l, previous)
                            .await?
                            .ok_or(OrchestrationError::RecoveryRequired)?;
                        if !evidence.completed {
                            return Err(OrchestrationError::RecoveryRequired);
                        }
                    }
                    let admitted = admission::live(c, l, now).await?;
                    let total = used
                        .checked_add(maximum)
                        .ok_or(OrchestrationError::LimitExceeded)?;
                    if total > admitted.permit.maximum.ai_api.minor_units() {
                        return Err(OrchestrationError::LimitExceeded);
                    }
                    // Existing scope check already verifies all four live holds.
                    one(
                        sqlx::query("INSERT INTO coding_agent_calls VALUES(?,?,?,?,?,?)")
                            .bind(l.task.plan.project_id.as_str())
                            .bind(l.attempt_id.as_str())
                            .bind(i64::from(step))
                            .bind(encode(&call, 4096)?)
                            .bind(self.coding_authority()?.seal(b"provider-call", &call)?)
                            .bind(now)
                            .execute(&mut *c)
                            .await?
                            .rows_affected(),
                    )?;
                    execute = true;
                }
                call
            }
            AgentOperation::Finish { evidence, .. } => {
                let call = calls
                    .get(usize::from(step) - 1)
                    .ok_or(OrchestrationError::MissingArtifact)?;
                if (evidence.completed && evidence.response_sha256.is_none())
                    || call.adapter != evidence.adapter
                    || [evidence.usage.input_tokens, evidence.usage.output_tokens]
                        .iter()
                        .flatten()
                        .any(|v| *v > 1_000_000_000)
                {
                    return Err(OrchestrationError::InvalidArtifact);
                }
                if let Some(old) = self.agent_observation(c, l, call).await? {
                    same(&old, evidence)?;
                } else {
                    one(
                        sqlx::query("INSERT INTO coding_agent_observations VALUES(?,?,?,?,?,?)")
                            .bind(l.task.plan.project_id.as_str())
                            .bind(l.attempt_id.as_str())
                            .bind(i64::from(step))
                            .bind(encode(evidence, 4096)?)
                            .bind(
                                self.coding_authority()?
                                    .seal(b"provider-observation", &(call, evidence))?,
                            )
                            .bind(now)
                            .execute(&mut *c)
                            .await?
                            .rows_affected(),
                    )?;
                }
                call.clone()
            }
        };
        Ok(StepReceipt {
            operation: commitment(&call)?,
            max_output_tokens: 8192,
            authorized_minor: call.maximum,
            execute,
        })
    }
    async fn agent_observation(
        &self,
        c: &mut SqliteConnection,
        l: &LeaseRef,
        call: &Call,
    ) -> Result<Option<ExecutionEvidence>, OrchestrationError> {
        let row=sqlx::query("SELECT body,seal FROM coding_agent_observations WHERE project_id=? AND attempt_id=? AND step=?").bind(l.task.plan.project_id.as_str()).bind(l.attempt_id.as_str()).bind(i64::from(call.step)).fetch_optional(c).await?;
        row.map(|r| {
            let e: ExecutionEvidence = decode(&r.get::<Vec<u8>, _>(0), 4096)?;
            self.coding_authority()?.verify(
                b"provider-observation",
                &(call, &e),
                &r.get::<Vec<u8>, _>(1),
            )?;
            Ok(e)
        })
        .transpose()
    }
    #[cfg(test)]
    pub(crate) fn fixture_agent_enable(&mut self, maximum: i64) {
        self.coding.as_mut().unwrap().agent_maximum = Some(maximum);
    }
}
