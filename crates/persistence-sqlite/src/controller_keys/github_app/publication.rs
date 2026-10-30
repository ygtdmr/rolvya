use super::*;
use crate::{admission::LeaseRef, orchestration::SyntheticCoordinator};
use base64::{Engine, engine::general_purpose::STANDARD};
use rolvya_domain::ids::ProjectId;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeMap;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PublicationState {
    Prepared,
    Submitting,
    Published,
    Unknown,
    Blocked,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublicationReceipt {
    pub operation: String,
    pub reference: String,
    pub commit: GitSha,
    pub state: PublicationState,
}
/// Not caller-constructible or deserializable publicly. Current R09 validator
/// supports exact synthetic artifact bytes only; arbitrary code bytes fail closed.
#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct AcceptedChange {
    lease: LeaseRef,
    acceptance: String,
    fingerprint: String,
    path: String,
    pub(super) bytes: Vec<u8>,
}
#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct Entry {
    path: String,
    mode: String,
    #[serde(rename = "type")]
    kind: String,
    sha: GitSha,
}
#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct Tree {
    sha: GitSha,
    entries: Vec<Entry>,
}
#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(super) struct Intent {
    installation: String,
    repository: RepositoryId,
    github_installation: InstallationId,
    binding_version: i64,
    operation: String,
    target: String,
    pub(super) lease: LeaseRef,
    base: GitSha,
    pub(super) change: AcceptedChange,
    trees: Vec<Tree>,
    tree: GitSha,
    commit: GitSha,
    created_at: i64,
}
impl Intent {
    fn message(&self) -> String {
        format!(
            "Rolvya independently validated artifact {}\n",
            self.operation
        )
    }
    fn receipt(&self, state: PublicationState) -> PublicationReceipt {
        PublicationReceipt {
            operation: self.operation.clone(),
            reference: self.target.clone(),
            commit: self.commit.clone(),
            state,
        }
    }
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Event {
    kind: String,
    reference: Option<String>,
    commit: Option<GitSha>,
    number: Option<i64>,
}
/// Provider-neutral controller operation port. No credential, arbitrary ref,
/// protected merge or worker-controlled publication method exists.
pub trait RemoteGit {
    fn read_pinned(
        &self,
        project: &ProjectId,
        base: &GitSha,
    ) -> impl std::future::Future<Output = Result<PinnedSource, GitError>> + Send;
    fn publish(
        &self,
        co: &SyntheticCoordinator,
        project: &ProjectId,
        operation: &str,
    ) -> impl std::future::Future<Output = Result<PublicationReceipt, GitError>> + Send;
    fn reconcile(
        &self,
        co: &SyntheticCoordinator,
        project: &ProjectId,
        operation: &str,
    ) -> impl std::future::Future<Output = Result<PublicationReceipt, GitError>> + Send;
}
impl GitHubRemote {
    pub(super) async fn token(
        &self,
        b: &consent::Consent,
        purpose: &str,
    ) -> Result<http::Token, GitError> {
        self.http.active()?;
        let now = self.now()?;
        let mut tx = self.store.pool.begin_with("BEGIN IMMEDIATE").await?;
        let jwt =
            super::super::secret_broker::github_consumer::jwt(&mut tx, &self.key, b, now).await?;
        one(sqlx::query("INSERT INTO git_credential_uses(project_id,binding_version,purpose,recorded_at) VALUES(?,?,?,?)").bind(b.binding.project.as_str()).bind(b.version).bind(purpose).bind(now).execute(&mut *tx).await?.rows_affected())?;
        tx.commit().await?;
        let token = self
            .http
            .token(&jwt, &b.binding, self.now()?, purpose)
            .await?;
        self.current_binding(&b.binding.project, b.version).await?;
        if self.now()? >= token.expires {
            return Err(GitError::Expired);
        }
        Ok(token)
    }
    async fn current_binding(
        &self,
        p: &ProjectId,
        version: i64,
    ) -> Result<consent::Consent, GitError> {
        let mut tx = self.store.pool.begin().await?;
        let b = consent::current(&mut tx, &self.key, p, self.now()?).await?;
        if b.version != version {
            return Err(GitError::Revoked);
        }
        tx.commit().await?;
        Ok(b)
    }
    pub async fn prepare(
        &self,
        co: &SyntheticCoordinator,
        l: &LeaseRef,
        base: &GitSha,
        path: &str,
    ) -> Result<PublicationReceipt, GitError> {
        self.http.active()?;
        GitSha::parse(base.as_str())?;
        let p = &l.task.plan.project_id;
        let mut tx = self.store.pool.begin().await?;
        let b = consent::current(&mut tx, &self.key, p, self.now()?).await?;
        model::allowed(&b.binding, path)?;
        let (bytes, acceptance, fingerprint) = co
            .publication_artifact_in(&mut tx, l)
            .await
            .map_err(|_| GitError::Unaccepted)?;
        tx.commit().await?;
        if bytes.len() > 65536 {
            return Err(GitError::ResourceLimit);
        }
        let change = AcceptedChange {
            lease: l.clone(),
            acceptance: acceptance.as_str().into(),
            fingerprint,
            path: path.into(),
            bytes,
        };
        let operation = hash(
            encode(&(
                hex(&self.key.metadata.installation.0),
                b.binding.repository,
                l,
                base,
                &change,
            ))?
            .as_bytes(),
        );
        if let Some(old) = self.load(p, &operation).await? {
            if old.binding_version != b.version || old.change != change || old.base != *base {
                return Err(GitError::Conflict);
            }
            self.check(co, &old).await?;
            return Ok(old.receipt(self.state(p, &operation).await?));
        }
        let token = self.token(&b, "read").await?;
        let source = self.http.source(&token, &b.binding, base).await?;
        let (trees, tree) = changed_trees(&source, &change)?;
        let created_at = self.now()?;
        let target = model::target(
            &hex(&self.key.metadata.installation.0),
            p.as_str(),
            l.task.task_id.as_str(),
            l.attempt_id.as_str(),
        );
        let mut i = Intent {
            installation: hex(&self.key.metadata.installation.0),
            repository: b.binding.repository,
            github_installation: b.binding.installation,
            binding_version: b.version,
            operation,
            target,
            lease: l.clone(),
            base: base.clone(),
            change,
            trees,
            tree,
            commit: base.clone(),
            created_at,
        };
        i.commit = model::object("commit", commit_bytes(&i)?.as_bytes());
        let text = encode(&i)?;
        self.checkpoint("intent-before");
        let mut tx = self.store.pool.begin_with("BEGIN IMMEDIATE").await?;
        self.check_in(&mut tx, co, &i).await?;
        if let Some(old) = load_in(&mut tx, &self.key, p, &i.operation).await? {
            if old.change != i.change
                || old.base != i.base
                || old.binding_version != i.binding_version
            {
                return Err(GitError::Conflict);
            }
            tx.commit().await?;
            return Ok(old.receipt(self.state(p, &i.operation).await?));
        }
        one(
            sqlx::query("INSERT INTO git_publications VALUES(?,?,?,?,?,?,?,?)")
                .bind(p.as_str())
                .bind(l.attempt_id.as_str())
                .bind(&i.operation)
                .bind(i.binding_version)
                .bind(i.repository.get())
                .bind(&i.target)
                .bind(&text)
                .bind(mac(&self.key, b"intent", &[text.as_bytes()]).as_slice())
                .execute(&mut *tx)
                .await?
                .rows_affected(),
        )?;
        event(
            &mut tx,
            &self.key,
            p,
            &i.operation,
            "prepared",
            None,
            None,
            None,
            self.now()?,
        )
        .await?;
        tx.commit().await?;
        self.checkpoint("intent-after");
        Ok(i.receipt(PublicationState::Prepared))
    }
    pub(super) async fn load(&self, p: &ProjectId, op: &str) -> Result<Option<Intent>, GitError> {
        let mut tx = self.store.pool.begin().await?;
        let r = load_in(&mut tx, &self.key, p, op).await?;
        tx.commit().await?;
        Ok(r)
    }
    async fn check_in(
        &self,
        c: &mut sqlx::SqliteConnection,
        co: &SyntheticCoordinator,
        i: &Intent,
    ) -> Result<(), GitError> {
        let b = consent::current(c, &self.key, &i.lease.task.plan.project_id, self.now()?).await?;
        if b.version != i.binding_version {
            return Err(GitError::Revoked);
        }
        if b.binding.repository != i.repository
            || b.binding.installation != i.github_installation
            || i.installation != hex(&self.key.metadata.installation.0)
            || i.target
                != model::target(
                    &i.installation,
                    i.lease.task.plan.project_id.as_str(),
                    i.lease.task.task_id.as_str(),
                    i.lease.attempt_id.as_str(),
                )
        {
            return Err(GitError::Denied);
        }
        model::allowed(&b.binding, &i.change.path)?;
        let (bytes, digest, fingerprint) = co
            .publication_artifact_in(c, &i.lease)
            .await
            .map_err(|_| GitError::StaleAttempt)?;
        if bytes != i.change.bytes
            || digest.as_str() != i.change.acceptance
            || fingerprint != i.change.fingerprint
            || i.change.lease != i.lease
        {
            return Err(GitError::Unaccepted);
        }
        Ok(())
    }
    pub(super) async fn check(
        &self,
        co: &SyntheticCoordinator,
        i: &Intent,
    ) -> Result<(), GitError> {
        let mut tx = self.store.pool.begin().await?;
        self.check_in(&mut tx, co, i).await?;
        tx.commit().await?;
        Ok(())
    }
    pub(super) async fn state(
        &self,
        p: &ProjectId,
        op: &str,
    ) -> Result<PublicationState, GitError> {
        let mut tx = self.store.pool.begin().await?;
        let states = events(&mut tx, &self.key, p, op).await?;
        let result = if states.iter().any(|e| e.kind == "blocked") {
            PublicationState::Blocked
        } else if states.iter().any(|e| e.kind == "published") {
            PublicationState::Published
        } else if states.iter().any(|e| e.kind == "unknown") {
            PublicationState::Unknown
        } else if states.iter().any(|e| e.kind == "submitting") {
            PublicationState::Submitting
        } else {
            PublicationState::Prepared
        };
        tx.commit().await?;
        Ok(result)
    }
    async fn record(
        &self,
        i: &Intent,
        kind: &str,
        reference: Option<&str>,
        number: Option<i64>,
    ) -> Result<(), GitError> {
        let mut tx = self.store.pool.begin_with("BEGIN IMMEDIATE").await?;
        event(
            &mut tx,
            &self.key,
            &i.lease.task.plan.project_id,
            &i.operation,
            kind,
            reference,
            Some(&i.commit),
            number,
            self.now()?,
        )
        .await?;
        tx.commit().await?;
        Ok(())
    }
    async fn boundary(
        &self,
        co: &SyntheticCoordinator,
        i: &Intent,
        t: &http::Token,
    ) -> Result<(), GitError> {
        if self.now()? >= t.expires {
            return Err(GitError::Expired);
        }
        self.check(co, i).await
    }
    pub(super) async fn remote_write(
        &self,
        co: &SyntheticCoordinator,
        i: &Intent,
        t: &http::Token,
    ) -> Result<(), GitError> {
        let prefix = format!("/repos/{}", t.location);
        self.boundary(co, i, t).await?;
        let blob = model::object("blob", &i.change.bytes);
        let reply = self
            .http
            .post(
                t,
                &format!("{prefix}/git/blobs"),
                json!({"content":STANDARD.encode(&i.change.bytes),"encoding":"base64"}),
            )
            .await?;
        if http::sha(&reply["sha"])? != blob {
            return Err(GitError::Integrity);
        }
        for tree in &i.trees {
            self.boundary(co, i, t).await?;
            let reply = self
                .http
                .post(
                    t,
                    &format!("{prefix}/git/trees"),
                    json!({"tree":tree.entries}),
                )
                .await?;
            if http::sha(&reply["sha"])? != tree.sha {
                return Err(GitError::Integrity);
            }
        }
        self.boundary(co, i, t).await?;
        let identity =
            json!({"name":"Rolvya","email":"rolvya@invalid","date":http::date(i.created_at)?});
        let commit=self.http.post(t,&format!("{prefix}/git/commits"),json!({"message":i.message(),"tree":i.tree,"parents":[i.base],"author":identity,"committer":identity})).await?;
        if http::sha(&commit["sha"])? != i.commit {
            return Err(GitError::Integrity);
        } // Final independent installation/repository permission recheck before ref visibility.
        let b = self
            .current_binding(&i.lease.task.plan.project_id, i.binding_version)
            .await?;
        let fresh = self.token(&b, "publication").await?;
        self.boundary(co, i, &fresh).await?;
        if self
            .http
            .reference(&fresh, &format!("refs/heads/{}", b.binding.source_branch))
            .await?
            != Some(i.base.clone())
        {
            return Err(GitError::Conflict);
        }
        let actual = self.http.reference(&fresh, &i.target).await?;
        // Remote lookups can outlive local authority. Recheck after them and
        // directly before making the branch visible; later races remain an
        // external uncertainty requiring the receipt-side check.
        self.boundary(co, i, &fresh).await?;
        if let Some(actual) = actual {
            return if actual == i.commit {
                Ok(())
            } else {
                Err(GitError::Conflict)
            };
        }
        self.checkpoint("effect-before");
        let reply = self
            .http
            .post(
                &fresh,
                &format!("/repos/{}/git/refs", fresh.location),
                json!({"ref":i.target,"sha":i.commit}),
            )
            .await?;
        self.checkpoint("effect-after");
        if reply["ref"] != i.target || http::sha(&reply["object"]["sha"])? != i.commit {
            return Err(GitError::Integrity);
        }
        Ok(())
    }
    pub async fn prepare_pull_request(
        &self,
        co: &SyntheticCoordinator,
        p: &ProjectId,
        op: &str,
    ) -> Result<PullRequestPreparation, GitError> {
        let i = self.load(p, op).await?.ok_or(GitError::Denied)?;
        self.check(co, &i).await?;
        if self.state(p, op).await? != PublicationState::Published {
            return Err(GitError::UnknownOutcome);
        }
        let b = self.current_binding(p, i.binding_version).await?;
        self.record(&i, "pr_prepared", None, None).await?;
        Ok(PullRequestPreparation {
            operation: i.operation,
            repository: i.repository,
            head: i
                .target
                .strip_prefix("refs/heads/")
                .ok_or(GitError::Integrity)?
                .into(),
            base: b.binding.source_branch,
            title: "Rolvya: independently validated task artifact".into(),
            body: format!(
                "Validated artifact evidence: {}. No merge authorization is granted.",
                i.change.acceptance
            ),
        })
    }
    pub async fn create_pull_request(
        &self,
        co: &SyntheticCoordinator,
        p: &ProjectId,
        op: &str,
    ) -> Result<i64, GitError> {
        let prepared = self.prepare_pull_request(co, p, op).await?;
        let i = self.load(p, op).await?.ok_or(GitError::Denied)?;
        let b = self.current_binding(p, i.binding_version).await?;
        let t = self.token(&b, "pull_request").await?;
        let mut tx = self.store.pool.begin_with("BEGIN IMMEDIATE").await?;
        self.check_in(&mut tx, co, &i).await?;
        let old = events(&mut tx, &self.key, p, op).await?;
        let attempted = old.iter().any(|e| e.kind == "pr_submitting");
        let existing = old
            .iter()
            .find(|e| e.kind == "pr_created")
            .and_then(|e| e.number);
        tx.commit().await?;
        if let Some(n) = existing {
            return Ok(n);
        }
        let list = self
            .http
            .get(
                &t,
                &format!(
                    "/repos/{}/pulls?state=all&head={}:{}&base={}",
                    t.location,
                    t.location.split('/').next().ok_or(GitError::Integrity)?,
                    prepared.head,
                    prepared.base
                ),
            )
            .await?;
        let matches = list.as_array().ok_or(GitError::Integrity)?;
        if matches.len() > 1 {
            return Err(GitError::Conflict);
        }
        if let Some(v) = matches.first() {
            validate_pr(v, &i, &prepared)?;
            let n = v["number"].as_i64().ok_or(GitError::Integrity)?;
            self.check(co, &i).await?;
            self.record(&i, "pr_created", None, Some(n)).await?;
            return Ok(n);
        }
        if attempted {
            return Err(GitError::UnknownOutcome);
        }
        let mut tx = self.store.pool.begin_with("BEGIN IMMEDIATE").await?;
        self.check_in(&mut tx, co, &i).await?;
        if events(&mut tx, &self.key, p, op)
            .await?
            .iter()
            .any(|e| e.kind == "pr_submitting")
        {
            return Err(GitError::UnknownOutcome);
        }
        event(
            &mut tx,
            &self.key,
            p,
            op,
            "pr_submitting",
            None,
            Some(&i.commit),
            None,
            self.now()?,
        )
        .await?;
        tx.commit().await?;
        self.boundary(co, &i, &t).await?;
        let reply=self.http.post(&t,&format!("/repos/{}/pulls",t.location),json!({"head":prepared.head,"base":prepared.base,"title":prepared.title,"body":prepared.body,"draft":true,"maintainer_can_modify":false})).await;
        let v = match reply {
            Ok(v) => v,
            Err(e) => {
                self.record(&i, "pr_unknown", None, None).await?;
                return Err(e);
            }
        };
        validate_pr(&v, &i, &prepared)?;
        self.check(co, &i).await?;
        let n = v["number"].as_i64().ok_or(GitError::Integrity)?;
        self.record(&i, "pr_created", None, Some(n)).await?;
        Ok(n)
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PullRequestPreparation {
    pub operation: String,
    pub repository: RepositoryId,
    pub head: String,
    pub base: String,
    pub title: String,
    pub body: String,
}
fn validate_pr(v: &Value, i: &Intent, p: &PullRequestPreparation) -> Result<(), GitError> {
    if v["head"]["repo"]["id"].as_i64() != Some(i.repository.get())
        || v["base"]["repo"]["id"].as_i64() != Some(i.repository.get())
        || v["head"]["ref"] != p.head
        || v["base"]["ref"] != p.base
        || http::sha(&v["head"]["sha"])? != i.commit
        || v["merged_at"].is_string()
    {
        return Err(GitError::Integrity);
    }
    if v["number"].as_i64().is_none_or(|n| n <= 0) {
        return Err(GitError::Integrity);
    }
    Ok(())
}
impl RemoteGit for GitHubRemote {
    async fn read_pinned(&self, p: &ProjectId, base: &GitSha) -> Result<PinnedSource, GitError> {
        self.http.active()?;
        let mut tx = self.store.pool.begin().await?;
        let b = consent::current(&mut tx, &self.key, p, self.now()?).await?;
        tx.commit().await?;
        let t = self.token(&b, "read").await?;
        let source = self.http.source(&t, &b.binding, base).await?;
        self.current_binding(p, b.version).await?;
        Ok(source)
    }
    async fn publish(
        &self,
        co: &SyntheticCoordinator,
        p: &ProjectId,
        op: &str,
    ) -> Result<PublicationReceipt, GitError> {
        self.http.active()?;
        let i = self.load(p, op).await?.ok_or(GitError::Denied)?;
        self.check(co, &i).await?;
        let state = self.state(p, op).await?;
        if state != PublicationState::Prepared {
            return self.reconcile(co, p, op).await;
        }
        let b = self.current_binding(p, i.binding_version).await?;
        let t = self.token(&b, "publication").await?;
        self.checkpoint("submit-before");
        let mut tx = self.store.pool.begin_with("BEGIN IMMEDIATE").await?;
        self.check_in(&mut tx, co, &i).await?;
        if events(&mut tx, &self.key, p, op)
            .await?
            .iter()
            .any(|e| e.kind == "submitting")
        {
            return Err(GitError::UnknownOutcome);
        }
        event(
            &mut tx,
            &self.key,
            p,
            op,
            "submitting",
            None,
            Some(&i.commit),
            None,
            self.now()?,
        )
        .await?;
        tx.commit().await?;
        self.checkpoint("submit-after");
        let result = self.remote_write(co, &i, &t).await;
        if let Err(e) = result {
            let kind = if matches!(e, GitError::Revoked | GitError::Denied | GitError::Conflict) {
                "blocked"
            } else {
                "unknown"
            };
            self.record(&i, kind, None, None).await?;
            return Err(e);
        }
        self.reconcile(co, p, op).await
    }
    async fn reconcile(
        &self,
        co: &SyntheticCoordinator,
        p: &ProjectId,
        op: &str,
    ) -> Result<PublicationReceipt, GitError> {
        self.http.active()?;
        let i = self.load(p, op).await?.ok_or(GitError::Denied)?;
        self.check(co, &i).await?;
        let b = self.current_binding(p, i.binding_version).await?;
        let t = self.token(&b, "read").await?;
        let actual = self.http.reference(&t, &i.target).await?;
        let Some(actual) = actual else {
            self.record(&i, "unknown", None, None).await?;
            return Ok(i.receipt(PublicationState::Unknown));
        };
        if actual != i.commit {
            self.record(&i, "blocked", Some(actual.as_str()), None)
                .await?;
            return Err(GitError::Conflict);
        }
        let remote = self
            .http
            .get(
                &t,
                &format!("/repos/{}/git/commits/{}", t.location, i.commit.as_str()),
            )
            .await?;
        if http::sha(&remote["sha"])? != i.commit
            || http::sha(&remote["tree"]["sha"])? != i.tree
            || remote["parents"].as_array().is_none_or(|p| p.len() != 1)
            || http::sha(&remote["parents"][0]["sha"])? != i.base
            || remote["message"] != i.message()
        {
            return Err(GitError::Integrity);
        }
        self.checkpoint("receipt-before");
        let mut tx = self.store.pool.begin_with("BEGIN IMMEDIATE").await?;
        if let Err(e) = self.check_in(&mut tx, co, &i).await {
            event(
                &mut tx,
                &self.key,
                p,
                op,
                "blocked",
                Some(&i.target),
                Some(&i.commit),
                None,
                self.now()?,
            )
            .await?;
            tx.commit().await?;
            return Err(e);
        }
        let old = events(&mut tx, &self.key, p, op).await?;
        if !old.iter().any(|e| e.kind == "published") {
            event(
                &mut tx,
                &self.key,
                p,
                op,
                "published",
                Some(&i.target),
                Some(&i.commit),
                None,
                self.now()?,
            )
            .await?;
        }
        tx.commit().await?;
        self.checkpoint("receipt-after");
        Ok(i.receipt(PublicationState::Published))
    }
}
async fn load_in(
    c: &mut sqlx::SqliteConnection,
    k: &ControllerKey,
    p: &ProjectId,
    op: &str,
) -> Result<Option<Intent>, GitError> {
    if op.len() != 64 || !op.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(GitError::InvalidInput);
    }
    let r = sqlx::query(
        "SELECT record,record_mac FROM git_publications WHERE project_id=? AND operation_id=?",
    )
    .bind(p.as_str())
    .bind(op)
    .fetch_optional(c)
    .await?;
    let Some(r) = r else { return Ok(None) };
    let text: &str = r.try_get("record")?;
    if !equal(
        r.try_get("record_mac")?,
        &mac(k, b"intent", &[text.as_bytes()]),
    ) {
        return Err(GitError::Integrity);
    }
    let i: Intent = decode(text)?;
    if i.operation != op
        || i.lease.task.plan.project_id != *p
        || i.installation != hex(&k.metadata.installation.0)
    {
        return Err(GitError::Integrity);
    }
    Ok(Some(i))
}
async fn events(
    c: &mut sqlx::SqliteConnection,
    k: &ControllerKey,
    p: &ProjectId,
    op: &str,
) -> Result<Vec<Event>, GitError> {
    let rows=sqlx::query("SELECT * FROM git_publication_events WHERE project_id=? AND operation_id=? ORDER BY revision LIMIT 257").bind(p.as_str()).bind(op).fetch_all(c).await?;
    if rows.len() > 256 {
        return Err(GitError::ResourceLimit);
    }
    let mut out = Vec::new();
    for (n, r) in rows.into_iter().enumerate() {
        let revision = r.try_get::<i64, _>("revision")?;
        let kind: &str = r.try_get("kind")?;
        let text: &str = r.try_get("record")?;
        let at = r.try_get::<i64, _>("recorded_at")?;
        if revision != n as i64 + 1
            || !equal(
                r.try_get("record_mac")?,
                &mac(
                    k,
                    b"event",
                    &[
                        p.as_str().as_bytes(),
                        op.as_bytes(),
                        &revision.to_be_bytes(),
                        kind.as_bytes(),
                        text.as_bytes(),
                        &at.to_be_bytes(),
                    ],
                ),
            )
        {
            return Err(GitError::Integrity);
        }
        let e: Event = decode(text)?;
        if e.kind != kind {
            return Err(GitError::Integrity);
        }
        out.push(e);
    }
    Ok(out)
}
#[allow(clippy::too_many_arguments)]
pub(super) async fn event(
    c: &mut sqlx::SqliteConnection,
    k: &ControllerKey,
    p: &ProjectId,
    op: &str,
    kind: &str,
    reference: Option<&str>,
    commit: Option<&GitSha>,
    number: Option<i64>,
    now: i64,
) -> Result<(), GitError> {
    let n:i64=sqlx::query_scalar("SELECT coalesce(max(revision),0)+1 FROM git_publication_events WHERE project_id=? AND operation_id=?").bind(p.as_str()).bind(op).fetch_one(&mut *c).await?;
    if n > 256 {
        return Err(GitError::ResourceLimit);
    }
    let text = encode(&Event {
        kind: kind.into(),
        reference: reference.map(str::to_owned),
        commit: commit.cloned(),
        number,
    })?;
    let tag = mac(
        k,
        b"event",
        &[
            p.as_str().as_bytes(),
            op.as_bytes(),
            &n.to_be_bytes(),
            kind.as_bytes(),
            text.as_bytes(),
            &now.to_be_bytes(),
        ],
    );
    one(
        sqlx::query("INSERT INTO git_publication_events VALUES(?,?,?,?,?,?,?)")
            .bind(p.as_str())
            .bind(op)
            .bind(n)
            .bind(kind)
            .bind(text)
            .bind(tag.as_slice())
            .bind(now)
            .execute(c)
            .await?
            .rows_affected(),
    )
}
fn commit_bytes(i: &Intent) -> Result<String, GitError> {
    http::date(i.created_at)?;
    Ok(format!(
        "tree {}\nparent {}\nauthor Rolvya <rolvya@invalid> {} +0000\ncommitter Rolvya <rolvya@invalid> {} +0000\n\n{}",
        i.tree.as_str(),
        i.base.as_str(),
        i.created_at,
        i.created_at,
        i.message()
    ))
}
#[derive(Default)]
struct Node {
    files: BTreeMap<String, (String, GitSha)>,
    dirs: BTreeMap<String, Node>,
}
fn add(root: &mut Node, path: &str, mode: &str, sha: GitSha) -> Result<(), GitError> {
    let mut split = path.split('/').peekable();
    let mut n = root;
    let mut depth = 0;
    while let Some(part) = split.next() {
        depth += 1;
        if depth > 32 {
            return Err(GitError::ResourceLimit);
        }
        if split.peek().is_none() {
            if n.dirs.contains_key(part)
                || n.files
                    .insert(part.into(), (mode.into(), sha.clone()))
                    .is_some()
            {
                return Err(GitError::Conflict);
            }
        } else {
            if n.files.contains_key(part) {
                return Err(GitError::Conflict);
            }
            n = n.dirs.entry(part.into()).or_default();
        }
    }
    Ok(())
}
fn trees(n: &Node, out: &mut Vec<Tree>) -> Result<GitSha, GitError> {
    let mut entries: Vec<Entry> = n
        .files
        .iter()
        .map(|(p, (m, s))| Entry {
            path: p.clone(),
            mode: m.clone(),
            kind: "blob".into(),
            sha: s.clone(),
        })
        .collect();
    for (p, dir) in &n.dirs {
        let sha = trees(dir, out)?;
        entries.push(Entry {
            path: p.clone(),
            mode: "040000".into(),
            kind: "tree".into(),
            sha,
        });
    }
    entries.sort_by_key(|e| {
        if e.kind == "tree" {
            format!("{}/", e.path)
        } else {
            e.path.clone()
        }
    });
    let mut raw = Vec::new();
    for e in &entries {
        raw.extend_from_slice(if e.kind == "tree" {
            b"40000"
        } else {
            e.mode.as_bytes()
        });
        raw.push(b' ');
        raw.extend_from_slice(e.path.as_bytes());
        raw.push(0);
        raw.extend_from_slice(&http::unhex(e.sha.as_str())?);
    }
    let sha = model::object("tree", &raw);
    out.push(Tree {
        sha: sha.clone(),
        entries,
    });
    Ok(sha)
}
fn changed_trees(
    source: &PinnedSource,
    c: &AcceptedChange,
) -> Result<(Vec<Tree>, GitSha), GitError> {
    let mut root = Node::default();
    for f in &source.files {
        add(&mut root, &f.path, &f.mode, f.sha.clone())?;
    }
    if trees(&root, &mut Vec::new())? != source.tree {
        return Err(GitError::Integrity);
    }
    let mut patched: Vec<_> = source
        .files
        .iter()
        .filter(|f| f.path != c.path)
        .cloned()
        .collect();
    patched.push(SourceFile {
        path: c.path.clone(),
        mode: "100644".into(),
        sha: model::object("blob", &c.bytes),
        bytes: c.bytes.clone(),
    });
    let mut root = Node::default();
    for f in patched {
        add(&mut root, &f.path, &f.mode, f.sha)?;
    }
    let mut all = Vec::new();
    let sha = trees(&root, &mut all)?;
    Ok((all, sha))
}
