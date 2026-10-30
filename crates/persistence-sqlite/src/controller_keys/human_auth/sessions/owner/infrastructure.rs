use super::*;
use crate::controller_keys::worker_auth::aws_fargate::{AccountEnrollment, consent};
impl HumanAuth {
    /// Staging is not consent. The returned exact action must undergo the existing
    /// fresh primary/factor proof and atomic Owner-action execution.
    pub async fn prepare_infrastructure_account(
        &self,
        h: &SessionHandle,
        p: &ProjectId,
        r: &AccountEnrollment,
        c: &BrowserContext,
    ) -> Result<OwnerAction, AuthError> {
        let (mut tx, f, now) = self.browser_transaction(c, Operation::Write).await?;
        let s = self
            .project_scope(&mut tx, h, &f, p, ProjectPermission::Govern, now)
            .await?;
        let id = new_id()?;
        let digest = consent::stage(&mut tx, &self.key, s.user()?, p, r, now, &id)
            .await
            .map_err(|_| AuthError::Denied)?;
        self.session_checkpoint(
            c,
            Operation::Write,
            s.deadline,
            Checkpoint::BeforeOwnerCommit,
            (&f, now),
        )?;
        tx.commit().await?;
        Ok(OwnerAction {
            payload: Payload::AwsInfrastructure {
                proposal: id,
                digest,
            },
        })
    }
}
