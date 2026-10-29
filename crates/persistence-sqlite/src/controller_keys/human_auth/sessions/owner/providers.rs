use super::*;
use crate::controller_keys::provider_accounts::{self as registry, ConnectionChange};
impl HumanAuth {
    /// Preparing data is not consent. Execution still requires the existing fresh,
    /// exact-action, single-use Owner proof and final session/role checks.
    pub async fn prepare_provider_connection(
        &self,
        h: &SessionHandle,
        p: &ProjectId,
        change: &ConnectionChange,
        c: &BrowserContext,
    ) -> Result<OwnerAction, AuthError> {
        let (mut tx, f, now) = self.browser_transaction(c, Operation::Write).await?;
        let s = self
            .project_scope(&mut tx, h, &f, p, ProjectPermission::Govern, now)
            .await?;
        let id = new_id()?;
        let digest = registry::store::prepare(&mut tx, &self.key, s.user()?, p, change, now, &id)
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
            payload: Payload::ProviderConnection {
                proposal: id,
                digest,
            },
        })
    }
}
impl HumanAuth {
    /// Refresh independently verified provider facts only within existing consent.
    /// No network call holds a SQLite transaction. This cannot add project/model
    /// permissions or change account/credential identity.
    pub async fn verify_provider_account(
        &self,
        h: &SessionHandle,
        p: &ProjectId,
        request: &registry::ProviderVerificationRequest,
        provider: &registry::ProviderRegistry,
        c: &BrowserContext,
    ) -> Result<(), AuthError> {
        let id = &request.connection;
        let expected = request.expected_version;
        let evidence = &request.evidence;
        let (mut read, f, now) = self.browser_transaction(c, Operation::Write).await?;
        let s = self
            .project_scope(&mut read, h, &f, p, ProjectPermission::Govern, now)
            .await?;
        registry::store::verify_request_owner(&mut read, &self.key, id, expected, s.user()?, p)
            .await
            .map_err(|_| AuthError::Denied)?;
        read.commit().await?;
        let (r, e) = provider
            .staged_verification(id, expected, p, evidence, now)
            .await
            .map_err(|_| AuthError::Denied)?;
        let (mut tx, f, end) = self.browser_transaction(c, Operation::Write).await?;
        let s = self
            .project_scope(&mut tx, h, &f, p, ProjectPermission::Govern, end)
            .await?;
        if end < now {
            return Err(AuthError::Denied);
        }
        registry::store::verify_request_owner(&mut tx, &self.key, id, expected, s.user()?, p)
            .await
            .map_err(|_| AuthError::Denied)?;
        provider
            .apply_verification(&mut tx, p, &r, &e, evidence, end)
            .await
            .map_err(|_| AuthError::Denied)?;
        self.session_checkpoint(
            c,
            Operation::Write,
            s.deadline.min(e.deadline()),
            Checkpoint::BeforeOwnerCommit,
            (&f, end),
        )?;
        provider
            .recheck_verification(&mut tx, p, &r, &e, evidence, self.now()?)
            .await
            .map_err(|_| AuthError::Denied)?;
        tx.commit().await?;
        Ok(())
    }
}

impl HumanAuth {
    /// Project-scoped, bounded inventory. This cannot confer use permission.
    pub async fn provider_connections(
        &self,
        h: &SessionHandle,
        p: &ProjectId,
        after: Option<&registry::ConnectionId>,
        limit: u16,
        c: &BrowserContext,
    ) -> Result<Vec<registry::ConnectionMetadata>, AuthError> {
        if limit == 0 || limit > 100 {
            return Err(AuthError::InvalidInput);
        }
        let (mut tx, f, now) = self.browser_transaction(c, Operation::Read).await?;
        let s = self
            .project_scope(&mut tx, h, &f, p, ProjectPermission::Read, now)
            .await?;
        let result = registry::store::metadata(&mut tx, &self.key, p, after, limit)
            .await
            .map_err(|_| AuthError::Denied)?;
        self.session_checkpoint(
            c,
            Operation::Read,
            s.deadline,
            Checkpoint::BeforeOwnerCommit,
            (&f, now),
        )?;
        tx.commit().await?;
        Ok(result)
    }
}
