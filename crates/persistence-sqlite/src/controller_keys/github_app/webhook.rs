use super::*;
use rolvya_domain::ids::ProjectId;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WebhookOutcome {
    Unrelated,
    RecheckRequired,
    Revoked,
    Duplicate,
}
impl GitHubRemote {
    /// Adapter must supply the original bounded body. Header IDs are dedup hints,
    /// not signed privileges; no event can restore or expand a binding.
    pub async fn receive_webhook(
        &self,
        p: &ProjectId,
        delivery: &str,
        signature: &str,
        body: &[u8],
    ) -> Result<WebhookOutcome, GitError> {
        if body.len() > 262144
            || delivery.is_empty()
            || delivery.len() > 100
            || !delivery
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-')
        {
            return Err(GitError::ResourceLimit);
        }
        let tag = signature.strip_prefix("sha256=").ok_or(GitError::Denied)?;
        if tag.len() != 64 || !tag.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(GitError::Denied);
        }
        let tag = http::unhex(tag)?;
        let mut tx = self.store.pool.begin_with("BEGIN IMMEDIATE").await?;
        let b = consent::load(&mut tx, &self.key, p).await?;
        super::super::secret_broker::github_consumer::verify_webhook(
            &mut tx,
            &self.key,
            &b,
            self.now()?,
            body,
            &tag,
        )
        .await?;
        let digest = hash(body);
        let old:Option<String>=sqlx::query_scalar("SELECT body_sha256 FROM git_webhook_deliveries WHERE project_id=? AND binding_version=? AND delivery_id=?").bind(p.as_str()).bind(b.version).bind(delivery).fetch_optional(&mut *tx).await?;
        if let Some(old) = old {
            return if old == digest {
                Ok(WebhookOutcome::Duplicate)
            } else {
                Err(GitError::Conflict)
            };
        }
        // GitHub signs the body, not X-GitHub-Delivery. Replays with another
        // header (including after renewed consent with the same webhook key)
        // must not create a new restrictive effect.
        let prior=sqlx::query("SELECT d.*,b.record AS binding_record,b.record_mac AS binding_mac FROM git_webhook_deliveries d JOIN git_bindings b ON b.project_id=d.project_id AND b.version=d.binding_version WHERE d.project_id=? AND d.body_sha256=? LIMIT 257").bind(p.as_str()).bind(&digest).fetch_all(&mut *tx).await?;
        if prior.len() > 256 {
            return Err(GitError::ResourceLimit);
        }
        for row in prior {
            let text: &str = row.try_get("binding_record")?;
            if !equal(
                row.try_get("binding_mac")?,
                &mac(&self.key, b"consent", &[text.as_bytes()]),
            ) {
                return Err(GitError::Integrity);
            }
            let old: consent::Consent = decode(text)?;
            if old.binding.project != *p
                || old.version != row.try_get::<i64, _>("binding_version")?
            {
                return Err(GitError::Integrity);
            }
            verify_stored(&self.key, p, old.version, &row)?;
            if old.binding.app == b.binding.app
                && old.binding.installation == b.binding.installation
                && old.binding.webhook_secret == b.binding.webhook_secret
            {
                return Ok(WebhookOutcome::Duplicate);
            }
        }
        let v: serde_json::Value =
            serde_json::from_slice(body).map_err(|_| GitError::InvalidInput)?;
        let action = v["action"].as_str().ok_or(GitError::InvalidInput)?;
        let same = v["installation"]["id"].as_i64() == Some(b.binding.installation.get());
        let removed = v["repositories_removed"].as_array().is_some_and(|r| {
            r.iter()
                .any(|r| r["id"].as_i64() == Some(b.binding.repository.get()))
        });
        let repo = v["repository"]["id"].as_i64() == Some(b.binding.repository.get());
        let (kind, outcome) = if same && matches!(action, "deleted" | "suspend") || same && removed
        {
            ("revoked", WebhookOutcome::Revoked)
        } else if same || repo {
            ("recheck", WebhookOutcome::RecheckRequired)
        } else {
            ("unrelated", WebhookOutcome::Unrelated)
        };
        let now = self.now()?;
        let tag = mac(
            &self.key,
            b"webhook",
            &[
                p.as_str().as_bytes(),
                &b.version.to_be_bytes(),
                delivery.as_bytes(),
                digest.as_bytes(),
                kind.as_bytes(),
                &now.to_be_bytes(),
            ],
        );
        one(
            sqlx::query("INSERT INTO git_webhook_deliveries VALUES(?,?,?,?,?,?,?)")
                .bind(p.as_str())
                .bind(b.version)
                .bind(delivery)
                .bind(digest)
                .bind(kind)
                .bind(tag.as_slice())
                .bind(now)
                .execute(&mut *tx)
                .await?
                .rows_affected(),
        )?;
        tx.commit().await?;
        Ok(outcome)
    }
}

// Authentication of stored restrictive decisions is checked on every use too.
// This is not anti-rollback protection: restored snapshots require external reconciliation.
pub(super) async fn restricted(
    c: &mut sqlx::SqliteConnection,
    k: &ControllerKey,
    p: &ProjectId,
    version: i64,
) -> Result<bool, GitError> {
    let rows=sqlx::query("SELECT delivery_id,body_sha256,decision,record_mac,recorded_at FROM git_webhook_deliveries WHERE project_id=? AND binding_version=? ORDER BY delivery_id LIMIT 1025").bind(p.as_str()).bind(version).fetch_all(c).await?;
    if rows.len() > 1024 {
        return Err(GitError::ResourceLimit);
    }
    let mut restricted = false;
    for r in rows {
        restricted |= verify_stored(k, p, version, &r)?;
    }
    Ok(restricted)
}

fn verify_stored(
    k: &ControllerKey,
    p: &ProjectId,
    version: i64,
    r: &sqlx::sqlite::SqliteRow,
) -> Result<bool, GitError> {
    let delivery: &str = r.try_get("delivery_id")?;
    let digest: &str = r.try_get("body_sha256")?;
    let kind: &str = r.try_get("decision")?;
    let now: i64 = r.try_get("recorded_at")?;
    let expected = mac(
        k,
        b"webhook",
        &[
            p.as_str().as_bytes(),
            &version.to_be_bytes(),
            delivery.as_bytes(),
            digest.as_bytes(),
            kind.as_bytes(),
            &now.to_be_bytes(),
        ],
    );
    if !equal(r.try_get("record_mac")?, &expected) {
        return Err(GitError::Integrity);
    }
    Ok(kind != "unrelated")
}
