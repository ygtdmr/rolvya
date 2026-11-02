use super::*;
impl HumanAuth {
    pub(super) async fn panel_connections_in(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
        s: &Scope,
        source: &str,
        after: i64,
        limit: u16,
        now: i64,
    ) -> Result<PanelPage<PanelConnection>, AuthError> {
        if s.membership.role != ProjectRole::Owner {
            return Err(AuthError::Denied);
        }
        let p = &s.project;
        let mut out = Vec::new();
        match source {
            "model" => {
                let rows = crate::controller_keys::provider_accounts::store::panel_metadata(
                    tx, &self.key, p, after, limit,
                )
                .await
                .map_err(|_| AuthError::Unavailable)?;
                for (n, r) in rows {
                    let (provider, label) = match (r.provider.as_str(), r.mode) {
                        ("openai", rolvya_domain::plan::AccessMode::ApiKey) => {
                            ("openai", "OpenAI API")
                        }
                        ("openai", rolvya_domain::plan::AccessMode::CliSession) => {
                            ("openai", "Codex CLI")
                        }
                        ("anthropic", rolvya_domain::plan::AccessMode::ApiKey) => {
                            ("anthropic", "Anthropic API")
                        }
                        ("anthropic", rolvya_domain::plan::AccessMode::CliSession) => {
                            ("anthropic", "Claude Code CLI")
                        }
                        _ => ("other", "Model account"),
                    };
                    let status = if r.revoked {
                        "revoked"
                    } else if r.recorded_verification_deadline.is_some_and(|t| t <= now) {
                        "expired"
                    } else if r.recorded_verification_deadline.is_some() {
                        "recorded_verified"
                    } else {
                        "unverified"
                    };
                    out.push((n,PanelConnection{id:self.panel_connection_id(p,"model",r.connection.as_str()),provider,label,account:r.account,mode:if r.mode==rolvya_domain::plan::AccessMode::ApiKey{"api"}else{"cli"},status,scope:p.as_str().to_owned(),version:r.connection_version,reason:"Recorded metadata is not current execution authority. Production provider, isolation and spending checks remain required.",operational:false,models:r.consented_models,verified_until:r.recorded_verification_deadline}));
                }
            }
            "github" => {
                if after > 1
                    && let Some((repo, location, version, status, until)) =
                        crate::controller_keys::github_app::consent::panel_metadata(
                            tx, &self.key, p, now,
                        )
                        .await
                        .map_err(|_| AuthError::Unavailable)?
                {
                    out.push((1,PanelConnection{id:self.panel_connection_id(p,"github",&repo),provider:"github",label:"GitHub App",account:location,mode:"repository",status,scope:p.as_str().to_owned(),version,reason:"Owner consent is recorded. Live App installation, permissions and deployment remain operationally gated.",operational:false,models:vec![],verified_until:Some(until)}));
                }
            }
            "aws" => {
                let rows = crate::controller_keys::worker_auth::aws_fargate::store::panel_metadata(
                    tx, &self.key, p, after, limit,
                )
                .await
                .map_err(|_| AuthError::Unavailable)?;
                for (n, id, account, version, status, until) in rows {
                    out.push((n,PanelConnection{id:self.panel_connection_id(p,"aws",&id),provider:"aws",label:"AWS ECS Fargate",account,mode:"infrastructure",status:if status!="revoked"&&until<=now{"expired"}else{status},scope:p.as_str().to_owned(),version,reason:"Signed infrastructure registration does not prove effective IAM, remote isolation or a safe live spending bound.",operational:false,models:vec![],verified_until:Some(until)}));
                }
            }
            _ => return Err(AuthError::InvalidInput),
        }
        self.panel_page(p, s.user()?, &format!("connections/{source}"), out, limit)
    }
}
