//! Fixed-origin bounded HTTPS. A private fixture origin is compiled only in tests.
use super::*;
use base64::{Engine, engine::general_purpose::STANDARD};
use reqwest::{Method, StatusCode};
use serde_json::{Value, json};
use zeroize::Zeroizing;
pub(super) struct Api {
    client: Option<reqwest::Client>,
    origin: String,
}
pub(super) struct Token {
    value: Zeroizing<String>,
    pub expires: i64,
    pub location: String,
}
impl Api {
    pub fn disabled() -> Self {
        Self {
            client: None,
            origin: "https://api.github.com".into(),
        }
    }
    #[cfg(test)]
    pub fn fixture(origin: String) -> Self {
        let tls = rustls::ClientConfig::builder_with_provider(std::sync::Arc::new(
            rustls::crypto::ring::default_provider(),
        ))
        .with_safe_default_protocol_versions()
        .unwrap()
        .with_root_certificates(rustls::RootCertStore::empty())
        .with_no_client_auth();
        Self {
            client: Some(
                reqwest::Client::builder()
                    .no_proxy()
                    .use_preconfigured_tls(tls)
                    .redirect(reqwest::redirect::Policy::none())
                    .timeout(std::time::Duration::from_secs(5))
                    .build()
                    .unwrap(),
            ),
            origin,
        }
    }
    pub fn active(&self) -> Result<(), GitError> {
        if self.client.is_none() {
            Err(GitError::LiveIntegrationDisabled)
        } else {
            Ok(())
        }
    }
    async fn raw(
        &self,
        method: Method,
        path: &str,
        bearer: &str,
        body: Option<Value>,
    ) -> Result<Zeroizing<Vec<u8>>, GitError> {
        self.active()?;
        if !path.starts_with('/')
            || path.contains("..")
            || path.contains('#')
            || path.contains('\\')
        {
            return Err(GitError::InvalidInput);
        }
        let mut auth = reqwest::header::HeaderValue::from_str(&format!("Bearer {bearer}"))
            .map_err(|_| GitError::Integrity)?;
        auth.set_sensitive(true);
        let mut r = self
            .client
            .as_ref()
            .ok_or(GitError::LiveIntegrationDisabled)?
            .request(method, format!("{}{path}", self.origin))
            .header("authorization", auth)
            .header("accept", "application/vnd.github+json")
            .header("x-github-api-version", "2022-11-28")
            .header("user-agent", "rolvya-trusted-remote-git");
        if let Some(b) = body {
            r = r
                .body(serde_json::to_vec(&b).map_err(|_| GitError::InvalidInput)?)
                .header("content-type", "application/json");
        }
        let mut response = r.send().await.map_err(|_| GitError::UnknownOutcome)?;
        match response.status() {
            StatusCode::TOO_MANY_REQUESTS | StatusCode::FORBIDDEN
                if response.status() == StatusCode::TOO_MANY_REQUESTS
                    || response.headers().contains_key("retry-after")
                    || response
                        .headers()
                        .get("x-ratelimit-remaining")
                        .is_some_and(|h| h == "0") =>
            {
                return Err(GitError::Throttled);
            }
            StatusCode::NOT_FOUND => return Err(GitError::Unavailable),
            StatusCode::CONFLICT | StatusCode::UNPROCESSABLE_ENTITY => {
                return Err(GitError::Conflict);
            }
            s if s.is_server_error() => return Err(GitError::UnknownOutcome),
            s if !s.is_success() => return Err(GitError::Denied),
            _ => {}
        }
        if response
            .content_length()
            .is_some_and(|n| n > 2 * 1024 * 1024)
        {
            return Err(GitError::ResourceLimit);
        }
        let mut out = Zeroizing::new(Vec::new());
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|_| GitError::UnknownOutcome)?
        {
            if out.len() + chunk.len() > 2 * 1024 * 1024 {
                return Err(GitError::ResourceLimit);
            }
            out.extend_from_slice(&chunk);
        }
        Ok(out)
    }
    async fn json(
        &self,
        m: Method,
        p: &str,
        token: &str,
        b: Option<Value>,
    ) -> Result<Value, GitError> {
        let raw = self.raw(m, p, token, b).await?;
        serde_json::from_slice(&raw).map_err(|_| GitError::Integrity)
    }
    pub async fn token(
        &self,
        jwt: &str,
        b: &RepositoryBinding,
        now: i64,
        purpose: &str,
    ) -> Result<Token, GitError> {
        let installation = self
            .json(
                Method::GET,
                &format!("/app/installations/{}", b.installation.get()),
                jwt,
                None,
            )
            .await?;
        if installation["id"].as_i64() != Some(b.installation.get())
            || installation["app_id"].as_i64() != Some(b.app.get())
            || !installation.get("suspended_at").is_some_and(Value::is_null)
        {
            return Err(GitError::Revoked);
        }
        if installation["permissions"]
            != serde_json::to_value(&b.permissions).map_err(|_| GitError::InvalidInput)?
        {
            return Err(GitError::Denied);
        }
        let permissions = match purpose {
            "read" => json!({"contents":"read","metadata":"read"}),
            "publication" => json!({"contents":"write","metadata":"read"}),
            "pull_request" => json!({"contents":"read","metadata":"read","pull_requests":"write"}),
            _ => return Err(GitError::InvalidInput),
        };
        for (key, v) in permissions.as_object().ok_or(GitError::Integrity)? {
            let granted = installation["permissions"][key].as_str();
            if granted != v.as_str() && !(v == "read" && granted == Some("write")) {
                return Err(GitError::Denied);
            }
        }
        let raw = self
            .raw(
                Method::POST,
                &format!("/app/installations/{}/access_tokens", b.installation.get()),
                jwt,
                Some(json!({"repository_ids":[b.repository.get()],"permissions":permissions})),
            )
            .await?;
        #[derive(serde::Deserialize)]
        struct Reply<'a> {
            token: &'a str,
            expires_at: &'a str,
            permissions: serde_json::Map<String, Value>,
            repositories: Vec<Value>,
        }
        let reply: Reply<'_> = serde_json::from_slice(&raw).map_err(|_| GitError::Integrity)?;
        let expires = parse_time(reply.expires_at)?;
        if expires <= now
            || expires > now.checked_add(3600).ok_or(GitError::Integrity)?
            || reply.token.is_empty()
            || reply.token.len() > 16384
            || reply.token.bytes().any(|b| b.is_ascii_control())
            || Value::Object(reply.permissions) != permissions
            || reply.repositories.len() != 1
            || reply.repositories[0]["id"].as_i64() != Some(b.repository.get())
        {
            return Err(GitError::Integrity);
        }
        let location = reply.repositories[0]["full_name"]
            .as_str()
            .ok_or(GitError::Integrity)?
            .to_owned();
        model::location(&location)?;
        let token = Token {
            value: Zeroizing::new(reply.token.into()),
            expires,
            location,
        };
        let meta = self
            .get(&token, &format!("/repositories/{}", b.repository.get()))
            .await?;
        if meta["id"].as_i64() != Some(b.repository.get())
            || meta["full_name"].as_str() != Some(&token.location)
        {
            return Err(GitError::Integrity);
        }
        let actual = self
            .json(
                Method::GET,
                &format!("/repos/{}/installation", token.location),
                jwt,
                None,
            )
            .await?;
        if actual["id"].as_i64() != Some(b.installation.get())
            || actual["app_id"].as_i64() != Some(b.app.get())
        {
            return Err(GitError::Revoked);
        }
        Ok(token)
    }
    pub async fn get(&self, t: &Token, p: &str) -> Result<Value, GitError> {
        self.json(Method::GET, p, &t.value, None).await
    }
    pub async fn post(&self, t: &Token, p: &str, b: Value) -> Result<Value, GitError> {
        self.json(Method::POST, p, &t.value, Some(b)).await
    }
    pub async fn reference(&self, t: &Token, r: &str) -> Result<Option<GitSha>, GitError> {
        let short = r.strip_prefix("refs/").ok_or(GitError::InvalidInput)?;
        match self
            .get(t, &format!("/repos/{}/git/ref/{short}", t.location))
            .await
        {
            Ok(v) => {
                if v["ref"] != r || v["object"]["type"] != "commit" {
                    return Err(GitError::Integrity);
                }
                Ok(Some(sha(&v["object"]["sha"])?))
            }
            Err(GitError::Unavailable) => Ok(None),
            Err(e) => Err(e),
        }
    }
    pub async fn source(
        &self,
        t: &Token,
        b: &RepositoryBinding,
        expected: &GitSha,
    ) -> Result<PinnedSource, GitError> {
        let actual = self
            .reference(t, &format!("refs/heads/{}", b.source_branch))
            .await?
            .ok_or(GitError::Unavailable)?;
        if actual != *expected {
            return Err(GitError::Conflict);
        }
        let c = self
            .get(
                t,
                &format!("/repos/{}/git/commits/{}", t.location, expected.as_str()),
            )
            .await?;
        if sha(&c["sha"])? != *expected {
            return Err(GitError::Integrity);
        }
        let root = sha(&c["tree"]["sha"])?;
        let mut pending = vec![(String::new(), root.clone())];
        let mut files = Vec::new();
        let mut visited = 0usize;
        let mut total = 0usize;
        while let Some((prefix, id)) = pending.pop() {
            visited += 1;
            if visited > 256 {
                return Err(GitError::ResourceLimit);
            }
            let tree = self
                .get(
                    t,
                    &format!("/repos/{}/git/trees/{}", t.location, id.as_str()),
                )
                .await?;
            if sha(&tree["sha"])? != id || tree["truncated"] != false {
                return Err(GitError::Integrity);
            }
            let entries = tree["tree"].as_array().ok_or(GitError::Integrity)?;
            if files.len() + pending.len() + entries.len() > 256 {
                return Err(GitError::ResourceLimit);
            }
            let mut raw_entries = Vec::new();
            let mut names = std::collections::BTreeSet::new();
            for entry in entries {
                let name = entry["path"].as_str().ok_or(GitError::Integrity)?;
                source_path(name)?;
                if name.contains('/') || !names.insert(name.to_owned()) {
                    return Err(GitError::Integrity);
                }
                let full = format!("{prefix}{name}");
                source_path(&full)?;
                let mode = entry["mode"].as_str().ok_or(GitError::Integrity)?;
                let kind = entry["type"].as_str().ok_or(GitError::Integrity)?;
                let id = sha(&entry["sha"])?;
                let sort_name = if kind == "tree" {
                    format!("{name}/")
                } else {
                    name.to_owned()
                };
                raw_entries.push((sort_name, name.to_owned(), mode.to_owned(), id.clone()));
                match (mode, kind) {
                    ("040000", "tree") => pending.push((format!("{full}/"), id)),
                    ("100644", "blob") | ("100755", "blob") => {
                        let blob = self
                            .get(
                                t,
                                &format!("/repos/{}/git/blobs/{}", t.location, id.as_str()),
                            )
                            .await?;
                        if sha(&blob["sha"])? != id || blob["encoding"] != "base64" {
                            return Err(GitError::Integrity);
                        }
                        let content = blob["content"].as_str().ok_or(GitError::Integrity)?;
                        if content.len() > 100000 {
                            return Err(GitError::ResourceLimit);
                        }
                        let bytes = STANDARD
                            .decode(content.replace('\n', ""))
                            .map_err(|_| GitError::Integrity)?;
                        total += bytes.len();
                        if bytes.len() > 65536 || total > 1024 * 1024 {
                            return Err(GitError::ResourceLimit);
                        }
                        if blob["size"].as_u64() != Some(bytes.len() as u64)
                            || model::object("blob", &bytes) != id
                        {
                            return Err(GitError::Integrity);
                        }
                        files.push(SourceFile {
                            path: full,
                            mode: mode.into(),
                            sha: id,
                            bytes,
                        });
                    }
                    _ => return Err(GitError::Unsupported),
                }
            }
            raw_entries.sort_by(|a, b| a.0.cmp(&b.0));
            let mut raw = Vec::new();
            for (_, name, mode, id) in raw_entries {
                raw.extend_from_slice(if mode == "040000" {
                    b"40000"
                } else {
                    mode.as_bytes()
                });
                raw.push(b' ');
                raw.extend_from_slice(name.as_bytes());
                raw.push(0);
                raw.extend_from_slice(&unhex(id.as_str())?);
            }
            if model::object("tree", &raw) != id {
                return Err(GitError::Integrity);
            }
        }
        files.sort_by(|a, b| a.path.cmp(&b.path));
        Ok(PinnedSource {
            repository: b.repository,
            commit: expected.clone(),
            tree: root,
            files,
        })
    }
}
pub(super) fn sha(v: &Value) -> Result<GitSha, GitError> {
    GitSha::parse(v.as_str().ok_or(GitError::Integrity)?).map_err(|_| GitError::Integrity)
}
pub(super) fn parse_time(s: &str) -> Result<i64, GitError> {
    aws_smithy_types::DateTime::from_str(s, aws_smithy_types::date_time::Format::DateTime)
        .map(|t| t.secs())
        .map_err(|_| GitError::Integrity)
}
pub(super) fn date(t: i64) -> Result<String, GitError> {
    aws_smithy_types::DateTime::from_secs(t)
        .fmt(aws_smithy_types::date_time::Format::DateTime)
        .map_err(|_| GitError::InvalidInput)
}
pub(super) fn unhex(s: &str) -> Result<Vec<u8>, GitError> {
    if !s.len().is_multiple_of(2) {
        return Err(GitError::InvalidInput);
    }
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).map_err(|_| GitError::InvalidInput))
        .collect()
}
fn source_path(s: &str) -> Result<(), GitError> {
    if s.len() > 240
        || s.split('/')
            .any(|c| c.is_empty() || c == "." || c == ".." || c.eq_ignore_ascii_case(".git"))
        || !s
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-_. /".contains(&b))
    {
        return Err(GitError::InvalidInput);
    }
    Ok(())
}
