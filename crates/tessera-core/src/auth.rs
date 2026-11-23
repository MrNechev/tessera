//! Authentication: single-password login with signed session cookies, a token mode for the
//! desktop app, and limited "remote" sessions for phones used as presentation remotes.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use hmac::{Hmac, Mac};
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;

use crate::config::AuthMode;

pub const COOKIE_NAME: &str = "tessera_session";
/// Desktop app token cookie.
pub const TOKEN_COOKIE: &str = "tessera_token";
/// Desktop app token header.
pub const TOKEN_HEADER: &str = "x-tessera-token";
/// A limited session for a phone used as a presentation remote: it only opens the remote
/// channel of one document (no library, no documents API).
pub const REMOTE_COOKIE: &str = "tessera_remote";

pub struct Auth {
    mode: AuthMode,
    key: [u8; 32],
    max_age: Duration,
    failures: Mutex<HashMap<String, Vec<Instant>>>,
}

/// Constant-time string comparison.
pub fn ct_eq(a: &str, b: &str) -> bool {
    a.len() == b.len() && bool::from(a.as_bytes().ct_eq(b.as_bytes()))
}

/// A `Set-Cookie` header value like Go's `http.Cookie.String()`.
pub fn cookie(name: &str, value: &str, max_age: i64, secure: bool, same_site: &str) -> String {
    let mut s = format!("{name}={value}; Path=/");
    if max_age > 0 {
        s.push_str(&format!("; Max-Age={max_age}"));
    } else if max_age < 0 {
        s.push_str("; Max-Age=0");
    }
    s.push_str("; HttpOnly");
    if secure {
        s.push_str("; Secure");
    }
    s.push_str(&format!("; SameSite={same_site}"));
    s
}

/// Value of a cookie in a `Cookie` header.
pub fn cookie_value<'a>(header: &'a str, name: &str) -> Option<&'a str> {
    header.split(';').find_map(|kv| {
        let (k, v) = kv.trim().split_once('=')?;
        (k == name).then(|| v.trim_matches('"'))
    })
}

impl Auth {
    /// Loads (or creates) the signing secret in `<data>/session.secret`. The key also mixes
    /// in the password, so changing the password logs every browser out.
    pub fn new(
        data_dir: &Path,
        mode: AuthMode,
        secret: &str,
        max_age: Duration,
    ) -> anyhow::Result<Auth> {
        let mut secret = secret.to_string();
        if secret.is_empty() {
            let p = data_dir.join("session.secret");
            let mut b = std::fs::read(&p).unwrap_or_default();
            if b.len() < 32 {
                let mut raw = [0u8; 32];
                rand::RngCore::fill_bytes(&mut rand::rng(), &mut raw);
                b = hex::encode(raw).into_bytes();
                write_private(&p, &b)?;
            }
            secret = String::from_utf8_lossy(&b).trim().to_string();
        }
        let password = match &mode {
            AuthMode::Password(p) => p.as_str(),
            _ => "",
        };
        let key: [u8; 32] = Sha256::digest(format!("{secret}\0{password}").as_bytes()).into();
        Ok(Auth {
            mode,
            key,
            max_age,
            failures: Mutex::new(HashMap::new()),
        })
    }

    pub fn mode(&self) -> &AuthMode {
        &self.mode
    }

    /// Whether a login is required (Password mode with a non-empty password).
    pub fn password_enabled(&self) -> bool {
        matches!(&self.mode, AuthMode::Password(p) if !p.is_empty())
    }

    pub fn max_age(&self) -> Duration {
        self.max_age
    }

    fn sign(&self, payload: &str) -> String {
        let mut m = <Hmac<Sha256> as Mac>::new_from_slice(&self.key).expect("hmac key");
        m.update(payload.as_bytes());
        URL_SAFE_NO_PAD.encode(m.finalize().into_bytes())
    }

    /// Whether the request is authenticated (`cookies` = the Cookie header, `token_header`
    /// = the X-Tessera-Token header).
    pub fn valid(&self, cookies: &str, token_header: Option<&str>) -> bool {
        match &self.mode {
            AuthMode::None => true,
            AuthMode::Password(p) if p.is_empty() => true,
            AuthMode::Password(_) => {
                let Some(v) = cookie_value(cookies, COOKIE_NAME) else {
                    return false;
                };
                let Some((payload, sig)) = v.split_once('.') else {
                    return false;
                };
                if !ct_eq(sig, &self.sign(payload)) {
                    return false;
                }
                let Ok(ts) = payload.parse::<i64>() else {
                    return false;
                };
                let age = chrono::Utc::now().timestamp() - ts;
                (age as i128) < self.max_age.as_secs() as i128
            }
            AuthMode::Token(t) => {
                token_header.map(|h| ct_eq(h, t)).unwrap_or(false)
                    || cookie_value(cookies, TOKEN_COOKIE)
                        .map(|c| ct_eq(c, t))
                        .unwrap_or(false)
            }
        }
    }

    /// Whether `t` is the desktop token.
    pub fn token_matches(&self, t: &str) -> bool {
        matches!(&self.mode, AuthMode::Token(tok) if ct_eq(t, tok))
    }

    /// Whether `ip` failed more than 10 times in the last 10 minutes.
    fn too_many(&self, ip: &str) -> bool {
        let mut g = self.failures.lock().unwrap_or_else(|e| e.into_inner());
        let keep: Vec<Instant> = g
            .get(ip)
            .map(|v| {
                v.iter()
                    .copied()
                    .filter(|t| t.elapsed() < Duration::from_secs(600))
                    .collect()
            })
            .unwrap_or_default();
        let n = keep.len();
        g.insert(ip.to_string(), keep);
        n >= 10
    }

    fn fail(&self, ip: &str) {
        self.failures
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .entry(ip.to_string())
            .or_default()
            .push(Instant::now());
    }

    /// Checks the password. On success returns the `Set-Cookie` value; on failure the
    /// reason (after a short delay for wrong passwords).
    pub async fn login(&self, ip: &str, password: &str, secure: bool) -> Result<String, String> {
        if self.too_many(ip) {
            return Err("Too many attempts. Wait a few minutes.".into());
        }
        let ok = match &self.mode {
            AuthMode::Password(p) => ct_eq(password, p),
            _ => false,
        };
        if !ok {
            self.fail(ip);
            tokio::time::sleep(Duration::from_millis(400)).await;
            return Err("Wrong password.".into());
        }
        let payload = chrono::Utc::now().timestamp().to_string();
        let value = format!("{payload}.{}", self.sign(&payload));
        Ok(cookie(
            COOKIE_NAME,
            &value,
            self.max_age.as_secs() as i64,
            secure,
            "Lax",
        ))
    }

    /// The `Set-Cookie` value that logs out.
    pub fn logout_cookie(&self, secure: bool) -> String {
        cookie(COOKIE_NAME, "", -1, secure, "Lax")
    }

    /// A remote-control session cookie for `doc_id`.
    pub fn remote_cookie(&self, doc_id: &str, ttl: Duration, secure: bool) -> String {
        let exp = chrono::Utc::now().timestamp() + ttl.as_secs() as i64;
        let payload = format!("r|{doc_id}|{exp}");
        let value = format!(
            "{}.{}",
            URL_SAFE_NO_PAD.encode(&payload),
            self.sign(&payload)
        );
        cookie(REMOTE_COOKIE, &value, ttl.as_secs() as i64, secure, "Lax")
    }

    /// Whether the request carries a remote session for `doc_id`.
    pub fn valid_remote(&self, cookies: &str, doc_id: &str) -> bool {
        let Some(v) = cookie_value(cookies, REMOTE_COOKIE) else {
            return false;
        };
        let Some((enc, sig)) = v.split_once('.') else {
            return false;
        };
        let Ok(raw) = URL_SAFE_NO_PAD.decode(enc) else {
            return false;
        };
        let raw = String::from_utf8_lossy(&raw).into_owned();
        if !ct_eq(sig, &self.sign(&raw)) {
            return false;
        }
        let parts: Vec<&str> = raw.split('|').collect();
        if parts.len() != 3 || parts[0] != "r" || parts[1] != doc_id {
            return false;
        }
        parts[2]
            .parse::<i64>()
            .map(|exp| chrono::Utc::now().timestamp() < exp)
            .unwrap_or(false)
    }
}

fn write_private(p: &Path, b: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .mode(0o600)
        .open(p)?;
    f.write_all(b)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn password_session_and_remote() {
        let dir = tempfile::tempdir().unwrap();
        let a = Auth::new(
            dir.path(),
            AuthMode::Password("pw".into()),
            "",
            Duration::from_secs(3600),
        )
        .unwrap();
        assert!(dir.path().join("session.secret").exists());
        assert!(!a.valid("", None));
        assert!(a.login("1.2.3.4", "nope", false).await.is_err());
        let c = a.login("1.2.3.4", "pw", true).await.unwrap();
        assert!(
            c.starts_with("tessera_session=")
                && c.contains("; Max-Age=3600; HttpOnly; Secure; SameSite=Lax"),
            "{c}"
        );
        let v = c.split(';').next().unwrap();
        assert!(a.valid(v, None));
        assert!(!a.valid(&v.replace('.', ".x"), None));
        // same secret file, different password → old cookies invalid
        let b = Auth::new(
            dir.path(),
            AuthMode::Password("other".into()),
            "",
            Duration::from_secs(3600),
        )
        .unwrap();
        assert!(!b.valid(v, None));
        let rc = a.remote_cookie("doc1", Duration::from_secs(60), false);
        let rv = rc.split(';').next().unwrap();
        assert!(a.valid_remote(rv, "doc1"));
        assert!(!a.valid_remote(rv, "doc2"));
    }

    #[test]
    fn token_mode() {
        let dir = tempfile::tempdir().unwrap();
        let a = Auth::new(
            dir.path(),
            AuthMode::Token("t0k".into()),
            "",
            Duration::from_secs(60),
        )
        .unwrap();
        assert!(a.valid("", Some("t0k")));
        assert!(a.valid("a=b; tessera_token=t0k", None));
        assert!(!a.valid("tessera_token=bad", Some("bad")));
        assert!(!a.password_enabled());
    }
}
