use super::*;
use crate::controller_keys::github_app::{RepositoryBinding, consent};
impl HumanAuth {
    /// Preparing configuration is not consent. Existing fresh exact-action proof
    /// and atomic execution must subsequently consume this proposal.
    pub async fn prepare_git_repository(
        &self,
        h: &SessionHandle,
        p: &ProjectId,
        r: &RepositoryBinding,
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
            payload: Payload::GitRepository {
                proposal: id,
                digest,
            },
        })
    }
}
