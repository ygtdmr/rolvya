//! Exactly one browser governance vertical slice. No generic Owner capability.
use super::super::owner::{OwnerAction, OwnerActionId, OwnerChallengeId, OwnerProofRequest};
use super::*;
use rolvya_domain::{
    ids::ProjectId,
    money::{Currency, Money},
};
use serde::Deserialize;
#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub(super) struct Proposal {
    expected_version: u64,
    currency: String,
    previous: String,
    total: String,
    confirm: bool,
}
impl Proposal {
    fn action(&self) -> Result<OwnerAction, AuthError> {
        if !self.confirm
            || self.expected_version == 0
            || self.expected_version > 9_007_199_254_740_991
        {
            return Err(AuthError::InvalidInput);
        }
        let amount = |s: &str| -> Result<i64, AuthError> {
            if s.len() > 19 {
                return Err(AuthError::InvalidInput);
            }
            let n = s.parse::<i64>().map_err(|_| AuthError::InvalidInput)?;
            if n < 0 || n.to_string() != s {
                return Err(AuthError::InvalidInput);
            }
            Ok(n)
        };
        let currency = Currency::new(&self.currency).map_err(|_| AuthError::InvalidInput)?;
        OwnerAction::budget(
            self.expected_version,
            Money::new(amount(&self.previous)?, currency).map_err(|_| AuthError::InvalidInput)?,
            Money::new(amount(&self.total)?, currency).map_err(|_| AuthError::InvalidInput)?,
        )
    }
}
#[derive(Deserialize)]
#[serde(tag = "stage", rename_all = "snake_case", deny_unknown_fields)]
pub(super) enum Command {
    Prepare {
        proposal: Proposal,
    },
    Password {
        proposal: Proposal,
        action: String,
        password: String,
        totp: Option<String>,
    },
    Email {
        proposal: Proposal,
        action: String,
        kind: String,
    },
    EmailConfirm {
        proposal: Proposal,
        action: String,
        challenge: String,
        kind: String,
        secret: String,
        totp: Option<String>,
    },
    Execute {
        proposal: Proposal,
        action: String,
    },
}
impl Command {
    pub(super) fn transport(&self) -> Option<TransportOperation> {
        match self {
            Self::Password { .. } => Some(TransportOperation::Password),
            Self::Email { .. } => Some(TransportOperation::IssueEmail),
            Self::EmailConfirm { .. } => Some(TransportOperation::ConfirmEmail),
            _ => None,
        }
    }
    fn proposal(&self) -> &Proposal {
        match self {
            Self::Prepare { proposal }
            | Self::Password { proposal, .. }
            | Self::Email { proposal, .. }
            | Self::EmailConfirm { proposal, .. }
            | Self::Execute { proposal, .. } => proposal,
        }
    }
    fn id(&self) -> Result<Option<OwnerActionId>, AuthError> {
        match self {
            Self::Prepare { .. } => Ok(None),
            Self::Password { action, .. }
            | Self::Email { action, .. }
            | Self::EmailConfirm { action, .. }
            | Self::Execute { action, .. } => OwnerActionId::parse(action.clone()).map(Some),
        }
    }
}
fn kind(k: &str) -> Result<ChallengeKind, AuthError> {
    match k {
        "otp" => Ok(ChallengeKind::Otp),
        "magic_link" => Ok(ChallengeKind::MagicLink),
        _ => Err(AuthError::InvalidInput),
    }
}
impl HumanBrowserServer {
    pub(super) async fn budget_command(
        &self,
        h: &SessionHandle,
        p: &ProjectId,
        input: Command,
        c: &BrowserContext,
        client: &ClientContext,
    ) -> Result<Response<Body>, AuthError> {
        let expected = input.proposal().action()?;
        let id = input.id()?;
        let access = self
            .auth
            .panel_budget_action(h, p, id.as_ref(), Some(&expected), c, Operation::Write)
            .await?;
        let Some(access) = access else {
            return Ok(json(
                StatusCode::FORBIDDEN,
                &serde_json::json!({"schema":1,"error":"project_access_denied"}),
            ));
        };
        let action = match input {
            Command::Prepare { .. } => self.auth.prepare_panel_budget(h, p, &expected, c).await?,
            Command::Password { password, totp, .. } => {
                let action = id.ok_or(AuthError::InvalidInput)?;
                let proof = OwnerProofRequest {
                    session: h,
                    project: p,
                    action: &action,
                    confirmation: Confirmation::Affirmative,
                    context: c,
                };
                let code = totp.map(TotpCode::parse).transpose()?;
                let result = self
                    .auth
                    .prove_owner_password(
                        &proof,
                        &Password::new(password.into_bytes())?,
                        client,
                        code.as_ref(),
                    )
                    .await;
                if matches!(result, Err(AuthError::Denied)) {
                    return Ok(json(
                        StatusCode::UNPROCESSABLE_ENTITY,
                        &serde_json::json!({"schema":1,"error":"reauthentication_denied"}),
                    ));
                }
                result?;
                action
            }
            Command::Email { kind: method, .. } => {
                if !access.email {
                    return Err(AuthError::Unavailable);
                }
                let action = id.ok_or(AuthError::InvalidInput)?;
                let proof = OwnerProofRequest {
                    session: h,
                    project: p,
                    action: &action,
                    confirmation: Confirmation::Affirmative,
                    context: c,
                };
                let prepared = self
                    .auth
                    .prepare_owner_email(&proof, kind(&method)?, client)
                    .await?;
                let reference = prepared.reference().as_str().to_owned();
                self.auth.dispatch_owner_email(prepared).await?;
                return Ok(json(
                    StatusCode::OK,
                    &serde_json::json!({"schema":1,"state":"email_pending","action":action.as_str(),"challenge":reference}),
                ));
            }
            Command::EmailConfirm {
                challenge,
                kind: method,
                secret,
                totp,
                ..
            } => {
                if !access.email {
                    return Err(AuthError::Unavailable);
                }
                let action = id.ok_or(AuthError::InvalidInput)?;
                let proof = OwnerProofRequest {
                    session: h,
                    project: p,
                    action: &action,
                    confirmation: Confirmation::Affirmative,
                    context: c,
                };
                let code = totp.map(TotpCode::parse).transpose()?;
                self.auth
                    .redeem_owner_email(
                        &proof,
                        &OwnerChallengeId::parse(challenge)?,
                        &ChallengeSecret::parse(secret)?,
                        kind(&method)?,
                        client,
                        code.as_ref(),
                    )
                    .await?;
                action
            }
            Command::Execute { .. } => {
                let action = id.ok_or(AuthError::InvalidInput)?;
                let receipt = self
                    .auth
                    .execute_owner_action(h, p, &action, &expected, Confirmation::Affirmative, c)
                    .await?;
                return Ok(json(
                    StatusCode::OK,
                    &serde_json::json!({"schema":1,"state":"committed","action":receipt.action.as_str(),"financialSequence":receipt.financial_sequence}),
                ));
            }
        };
        let value = self
            .auth
            .panel_budget_action(h, p, Some(&action), None, c, Operation::Write)
            .await?
            .ok_or(AuthError::Denied)?;
        let mut value = serde_json::to_value(value).map_err(|_| AuthError::Storage)?;
        value["schema"] = serde_json::json!(1);
        value["serverNow"] = serde_json::json!(self.auth.now()?);
        Ok(json(StatusCode::OK, &value))
    }
    pub(super) async fn budget_status(
        &self,
        h: &SessionHandle,
        p: &ProjectId,
        id: &str,
        c: &BrowserContext,
    ) -> Result<Response<Body>, AuthError> {
        let id = OwnerActionId::parse(id.to_owned())?;
        let Some(value) = self
            .auth
            .panel_budget_action(h, p, Some(&id), None, c, Operation::Read)
            .await?
        else {
            return Ok(json(
                StatusCode::FORBIDDEN,
                &serde_json::json!({"schema":1,"error":"project_access_denied"}),
            ));
        };
        let mut value = serde_json::to_value(value).map_err(|_| AuthError::Storage)?;
        value["schema"] = serde_json::json!(1);
        value["serverNow"] = serde_json::json!(self.auth.now()?);
        Ok(json(StatusCode::OK, &value))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn budget_proposal_rejects_overflow_ambiguous_amounts_and_unconfirmed_versions() {
        let base = serde_json::json!({"expectedVersion":1,"currency":"USD","previous":"20000","total":"25000","confirm":true});
        assert!(
            serde_json::from_value::<Proposal>(base.clone())
                .unwrap()
                .action()
                .is_ok()
        );
        for patch in [
            serde_json::json!({"total":"9223372036854775808"}),
            serde_json::json!({"total":"025000"}),
            serde_json::json!({"total":"25000.0"}),
            serde_json::json!({"total":"-1"}),
            serde_json::json!({"total":"20000"}),
            serde_json::json!({"confirm":false}),
            serde_json::json!({"expectedVersion":0}),
            serde_json::json!({"expectedVersion":9007199254740992u64}),
            serde_json::json!({"currency":"usd"}),
        ] {
            let mut value = base.clone();
            for (k, v) in patch.as_object().unwrap() {
                value[k] = v.clone();
            }
            assert!(
                serde_json::from_value::<Proposal>(value)
                    .unwrap()
                    .action()
                    .is_err()
            );
        }
        let mut unknown = base;
        unknown["owner"] = serde_json::json!(true);
        assert!(serde_json::from_value::<Proposal>(unknown).is_err());
    }
}
