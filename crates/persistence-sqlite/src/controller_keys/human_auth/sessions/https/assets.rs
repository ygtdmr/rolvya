use super::*;
use base64::Engine;
use std::path::Path;

/// Controller-host build artifacts, not remote/user supplied HTML. Deploy only
/// distributions that pass the existing parsed-HTML/AST production artifact gate.
pub struct BrowserAssets {
    files: BTreeMap<String, (Vec<u8>, &'static str)>,
    csp: header::HeaderValue,
}
impl BrowserAssets {
    pub async fn load(directory: &Path) -> Result<Self, AuthError> {
        let mut entries = tokio::fs::read_dir(directory)
            .await
            .map_err(|_| AuthError::Unavailable)?;
        let mut files = BTreeMap::new();
        let mut app = None;
        while let Some(entry) = entries
            .next_entry()
            .await
            .map_err(|_| AuthError::Unavailable)?
        {
            let name = entry
                .file_name()
                .to_str()
                .ok_or(AuthError::InvalidInput)?
                .to_owned();
            let kind = match name.as_str() {
                "index.html" => "text/html; charset=utf-8",
                "sw.js" => "application/javascript; charset=utf-8",
                "manifest.webmanifest" => "application/manifest+json",
                _ if name.starts_with("app.")
                    && name.ends_with(".js")
                    && name.len() <= 96
                    && name
                        .bytes()
                        .all(|c| c.is_ascii_alphanumeric() || b"._-".contains(&c))
                    && app.is_none() =>
                {
                    app = Some(name.clone());
                    "application/javascript; charset=utf-8"
                }
                _ => return Err(AuthError::InvalidInput),
            };
            let metadata = tokio::fs::symlink_metadata(entry.path())
                .await
                .map_err(|_| AuthError::Unavailable)?;
            let limit = if name.starts_with("app.") {
                4 * 1024 * 1024
            } else {
                40000
            };
            if !metadata.is_file() || metadata.len() > limit {
                return Err(AuthError::InvalidInput);
            }
            let bytes = tokio::fs::read(entry.path())
                .await
                .map_err(|_| AuthError::Unavailable)?;
            if bytes.len() as u64 > limit {
                return Err(AuthError::InvalidInput);
            }
            files.insert(format!("/{name}"), (bytes, kind));
        }
        if files.len() != 4 {
            return Err(AuthError::InvalidInput);
        }
        let app = app.ok_or(AuthError::InvalidInput)?;
        let html = std::str::from_utf8(&files.get("/index.html").ok_or(AuthError::InvalidInput)?.0)
            .map_err(|_| AuthError::InvalidInput)?;
        let csp = html
            .split_once("<meta http-equiv=\"Content-Security-Policy\" content=\"")
            .and_then(|(_, s)| s.split_once('"'))
            .ok_or(AuthError::InvalidInput)?
            .0
            .replace("&#39;", "'")
            .replace("&amp;", "&");
        let sri = base64::engine::general_purpose::STANDARD.encode(sha2::Sha384::digest(
            &files
                .get(&format!("/{app}"))
                .ok_or(AuthError::InvalidInput)?
                .0,
        ));
        if csp.contains("unsafe-")
            || !csp.contains("style-src-attr 'none'")
            || !csp.contains("connect-src 'self'")
            || !csp.contains("object-src 'none'")
            || !csp.contains("default-src 'none'")
            || !csp.contains("script-src 'self'")
            || !html.contains(&format!("integrity=\"sha384-{sri}\""))
            || !html.contains(&format!("src=\"/{app}\""))
        {
            return Err(AuthError::InvalidInput);
        }
        let csp = header::HeaderValue::from_str(&format!("{csp}; frame-ancestors 'none'"))
            .map_err(|_| AuthError::InvalidInput)?;
        Ok(Self { files, csp })
    }
    pub(super) fn response(&self, path: &str) -> Option<Response<Body>> {
        let name = if path == "/" { "/index.html" } else { path };
        let (bytes, mime) = self.files.get(name)?;
        let mut r = Response::new(Body::from(bytes.clone()));
        r.headers_mut()
            .insert(header::CONTENT_TYPE, header::HeaderValue::from_static(mime));
        r.headers_mut()
            .insert(header::CONTENT_SECURITY_POLICY, self.csp.clone());
        r.headers_mut().insert(
            header::X_CONTENT_TYPE_OPTIONS,
            header::HeaderValue::from_static("nosniff"),
        );
        r.headers_mut().insert(
            header::REFERRER_POLICY,
            header::HeaderValue::from_static("no-referrer"),
        );
        r.headers_mut().insert(
            header::X_FRAME_OPTIONS,
            header::HeaderValue::from_static("DENY"),
        );
        r.headers_mut().insert(
            "x-rolvya-static",
            header::HeaderValue::from_static("public-shell-v1"),
        );
        r.headers_mut().insert(
            header::CACHE_CONTROL,
            header::HeaderValue::from_static(if name.starts_with("/app.") {
                "public, max-age=31536000, immutable"
            } else {
                "public, max-age=0, must-revalidate"
            }),
        );
        Some(r)
    }
}
