use super::*;
use std::net::IpAddr;

#[derive(Clone)]
pub(super) struct Channel {
    pub peer: IpAddr,
    pub exporter: [u8; 32],
}
pub(super) struct Active {
    pub facts: BrowserFacts,
    pub operation: Operation,
    pub transport: Option<TransportOperation>,
    pub peer: IpAddr,
    pub expires: i64,
    pub evidence: RequestId,
    // Exact request correlation is internal, never supplied by the browser.
    pub commitment: [u8; 32],
    pub cookies: Option<Vec<header::HeaderValue>>,
    pub sent: bool,
}
#[derive(Default)]
pub(super) struct RequestAuthority {
    pub active: Mutex<BTreeMap<String, Active>>,
    windows: Mutex<BTreeMap<(IpAddr, u8), (i64, u16)>>,
}
pub(super) struct RequestGuard {
    authority: Arc<RequestAuthority>,
    pub context: BrowserContext,
}
impl Drop for RequestGuard {
    fn drop(&mut self) {
        self.authority.cancel(&self.context);
    }
}
impl RequestAuthority {
    pub fn cancel(&self, c: &BrowserContext) {
        if let Ok(mut active) = self.active.lock()
            && active
                .get(c.request_id.as_str())
                .is_some_and(|a| a.evidence == c.evidence)
        {
            active.remove(c.request_id.as_str());
        }
    }
    pub fn rate(&self, ip: IpAddr, now: i64, class: u8) -> Result<(), AuthError> {
        let mut windows = self.windows.lock().map_err(|_| AuthError::Unavailable)?;
        windows.retain(|_, v| now >= v.0 && now - v.0 < 60);
        let key = (ip, class);
        if windows.len() >= 3072 && !windows.contains_key(&key) {
            return Err(AuthError::Busy);
        }
        let v = windows.entry(key).or_insert((now, 0));
        // 16 live subscribers may each refresh three reads per second. Keep
        // these bounded reads separate from login/session mutation capacity.
        let limit = match class {
            1 => 4096,
            2 => 256,
            _ => 120,
        };
        if v.1 >= limit {
            return Err(AuthError::Busy);
        }
        v.1 += 1;
        Ok(())
    }
    pub fn reserve(self: &Arc<Self>, value: Active) -> Result<RequestGuard, AuthError> {
        let context = BrowserContext {
            request_id: RequestId::new(new_id()?).map_err(|_| AuthError::Randomness)?,
            evidence: value.evidence.clone(),
        };
        let mut active = self.active.lock().map_err(|_| AuthError::Unavailable)?;
        if active.len() >= 64 {
            return Err(AuthError::Busy);
        }
        active.insert(context.request_id.as_str().to_owned(), value);
        Ok(RequestGuard {
            authority: self.clone(),
            context,
        })
    }
    pub fn cookies(
        &self,
        c: &BrowserContext,
        now: i64,
    ) -> Result<Vec<header::HeaderValue>, AuthError> {
        let mut a = self.active.lock().map_err(|_| AuthError::Unavailable)?;
        let request = a.get_mut(c.request_id.as_str()).ok_or(AuthError::Denied)?;
        if request.evidence != c.evidence || request.sent || now >= request.expires {
            return Err(AuthError::Denied);
        }
        request.sent = true;
        Ok(request.cookies.take().unwrap_or_default())
    }
}
fn cookie(name: &str, value: &str, seconds: i64) -> Result<header::HeaderValue, AuthError> {
    let mut h = header::HeaderValue::from_str(&format!(
        "{name}={value}; Path=/; Secure; HttpOnly; SameSite=Strict; Max-Age={seconds}"
    ))
    .map_err(|_| AuthError::Denied)?;
    h.set_sensitive(true);
    Ok(h)
}
pub(super) fn clear_session() -> Result<Vec<header::HeaderValue>, AuthError> {
    Ok(vec![
        cookie("__Host-rolvya", "", 0)?,
        cookie("__Host-rolvya-csrf", "", 0)?,
        cookie("__Host-rolvya-factor", "", 0)?,
    ])
}
impl BrowserAuthority for RequestAuthority {
    fn verify(
        &self,
        c: &BrowserContext,
        operation: Operation,
        now: i64,
    ) -> Result<BrowserFacts, AuthError> {
        let all = self.active.lock().map_err(|_| AuthError::Unavailable)?;
        let a = all.get(c.request_id.as_str()).ok_or(AuthError::Denied)?;
        if a.evidence != c.evidence
            || a.operation != operation
            || now >= a.expires
            || a.commitment == [0; 32]
        {
            return Err(AuthError::Denied);
        }
        Ok(a.facts.clone())
    }
    fn deliver(&self, c: &BrowserContext, t: &SessionTicket, now: i64) -> Result<(), AuthError> {
        let mut all = self.active.lock().map_err(|_| AuthError::Unavailable)?;
        let a = all
            .get_mut(c.request_id.as_str())
            .ok_or(AuthError::Denied)?;
        if a.evidence != c.evidence
            || now >= a.expires
            || a.cookies.is_some()
            || a.sent
            || !matches!(
                a.operation,
                Operation::Issue | Operation::CompleteFactor | Operation::Renew
            )
        {
            return Err(AuthError::Denied);
        }
        a.cookies = Some(vec![
            cookie("__Host-rolvya", &t.handle.0, TOKEN_SECONDS)?,
            cookie("__Host-rolvya-csrf", &t.csrf, TOKEN_SECONDS)?,
            cookie("__Host-rolvya-factor", "", 0)?,
        ]);
        Ok(())
    }
    fn pending(&self, c: &BrowserContext, p: &PendingLogin, now: i64) -> Result<(), AuthError> {
        let mut all = self.active.lock().map_err(|_| AuthError::Unavailable)?;
        let a = all
            .get_mut(c.request_id.as_str())
            .ok_or(AuthError::Denied)?;
        if a.evidence != c.evidence
            || a.operation != Operation::Issue
            || now >= a.expires
            || a.cookies.is_some()
            || a.sent
        {
            return Err(AuthError::Denied);
        }
        a.cookies = Some(vec![cookie(
            "__Host-rolvya-factor",
            &format!("{}.{}", p.id, p.secret.as_str()),
            PROOF_SECONDS,
        )?]);
        Ok(())
    }
    fn provision(&self, _: &BrowserContext, _: &FactorEnrollment, _: i64) -> Result<(), AuthError> {
        Err(AuthError::Unavailable)
    }
    fn recovery(&self, _: &BrowserContext, _: &RecoveryBundle, _: i64) -> Result<(), AuthError> {
        Err(AuthError::Unavailable)
    }
}
impl TransportAuthority for RequestAuthority {
    fn verify(
        &self,
        c: &ClientContext,
        operation: TransportOperation,
        now: i64,
    ) -> Result<VerifiedTransport, AuthError> {
        let all = self.active.lock().map_err(|_| AuthError::Unavailable)?;
        let a = all.get(c.request_id.as_str()).ok_or(AuthError::Denied)?;
        if c.transport_evidence != a.evidence
            || a.transport != Some(operation)
            || now >= a.expires
            || !a.facts.preauth_csrf
        {
            return Err(AuthError::Denied);
        }
        Ok(VerifiedTransport {
            ip: a.peer,
            browser: a.facts.browser,
            parent: None,
        })
    }
}
