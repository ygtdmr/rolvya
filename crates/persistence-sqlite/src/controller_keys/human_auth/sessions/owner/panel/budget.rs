//! Browser presentation and narrowing of the existing exact-action engine.
use super::*;
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(in crate::controller_keys::human_auth) struct PanelBudgetAction {
    pub action: Option<String>,
    pub state: &'static str,
    pub password: bool,
    pub email: bool,
    pub totp: bool,
    pub proof_expires_at: Option<i64>,
    pub financial_sequence: Option<i64>,
}
impl HumanAuth {
    pub(in crate::controller_keys::human_auth) async fn panel_budget_action(
        &self,
        h: &SessionHandle,
        p: &ProjectId,
        id: Option<&OwnerActionId>,
        expected: Option<&OwnerAction>,
        c: &BrowserContext,
        op: Operation,
    ) -> Result<Option<PanelBudgetAction>, AuthError> {
        let (mut tx, f, now) = self.browser_transaction(c, op).await?;
        // Separate session denial from project/role denial without reading a
        // protected financial or action record before current Owner authority.
        self.session_row(&mut tx, h, &f, now, false).await?;
        let s = match self
            .project_scope(&mut tx, h, &f, p, ProjectPermission::Read, now)
            .await
        {
            Ok(s) => s,
            Err(AuthError::Denied) => return Ok(None),
            Err(e) => return Err(e),
        };
        if s.membership.role != ProjectRole::Owner {
            return Ok(None);
        }
        let (_, policy) = policy(&mut tx).await?;
        let flags=sqlx::query("SELECT factor_required,EXISTS(SELECT 1 FROM human_passwords p WHERE p.user_id=u.user_id) AS password,EXISTS(SELECT 1 FROM human_emails e WHERE e.user_id=u.user_id AND e.verified=1 AND e.email_login=1) AS email FROM human_users u WHERE u.user_id=?")
            .bind(s.user()?).fetch_one(&mut *tx).await?;
        let password = policy.password_enabled() && flags.try_get::<bool, _>("password")?;
        let email = !password
            && policy.email_enabled()
            && flags.try_get::<bool, _>("email")?
            && self.mail_ready();
        let mut result = PanelBudgetAction {
            action: id.map(|i| i.as_str().to_owned()),
            state: "available",
            password,
            email,
            totp: flags.try_get("factor_required")?,
            proof_expires_at: None,
            financial_sequence: None,
        };
        if let Some(id) = id {
            let r =
                sqlx::query("SELECT * FROM human_owner_actions WHERE project_id=? AND action_id=?")
                    .bind(p.as_str())
                    .bind(id.as_str())
                    .fetch_optional(&mut *tx)
                    .await?
                    .ok_or(AuthError::Denied)?;
            if r.try_get::<&str, _>("actor")? != s.user()? {
                return Err(AuthError::Denied);
            }
            if !constant_eq(
                &self.action_binding(&r)?,
                &r.try_get::<Vec<u8>, _>("binding")?,
            ) {
                return Err(AuthError::Denied);
            }
            let action = OwnerAction::decode(r.try_get("payload")?)?;
            if !matches!(action.payload, Payload::Budget { .. }) {
                return Err(AuthError::Denied);
            }
            if let Some(expected) = expected {
                if &action != expected {
                    return Err(AuthError::Denied);
                }
                self.action_in(&mut tx, id, &s, now).await?;
            }
            result.state = match r.try_get::<&str, _>("state")? {
                "pending" => "pending",
                "committed" => "committed",
                "revoked" => "revoked",
                "held" => "held",
                _ => return Err(AuthError::Storage),
            };
            let proof = sqlx::query("SELECT * FROM human_owner_proofs WHERE action_id=?")
                .bind(id.as_str())
                .fetch_optional(&mut *tx)
                .await?;
            if let Some(proof) = proof {
                if !constant_eq(
                    &self.proof_binding(&r, &proof)?,
                    &proof.try_get::<Vec<u8>, _>("binding")?,
                ) {
                    return Err(AuthError::Denied);
                }
                result.proof_expires_at = Some(proof.try_get("expires_at")?);
            }
            let effect =
                sqlx::query("SELECT financial_sequence FROM human_owner_effects WHERE action_id=?")
                    .bind(id.as_str())
                    .fetch_optional(&mut *tx)
                    .await?;
            if (result.state == "committed") != effect.is_some() {
                return Err(AuthError::Storage);
            }
            result.financial_sequence = effect
                .map(|r| r.try_get("financial_sequence"))
                .transpose()?
                .flatten();
        }
        self.session_checkpoint(
            c,
            op,
            s.deadline,
            Checkpoint::BeforeSessionCommit,
            (&f, now),
        )?;
        tx.commit().await?;
        Ok(Some(result))
    }
}
