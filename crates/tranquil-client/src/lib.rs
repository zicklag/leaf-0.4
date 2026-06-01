//! Wrapper around jacquard for managing AT Protocol accounts via an admin.
//!
//! 1. [`TranquilClient::login`] — admin logs in.
//! 2. Admin creates invite codes, creates accounts via `createAccount`.
//! 3. [`login_as`](TranquilClient::login_as) — admin sets password then logs in
//!    as any account, returns a fresh jacquard [`Agent`].
//! 4. Use [`AgentSessionExt`] on the returned agent for CRUD.

pub mod error;

use jacquard::client::MemoryCredentialSession;
use jacquard::client::Agent;
use jacquard::common::CowStr;
pub use jacquard::client::AgentSessionExt;
pub use error::Error;

/// Admin client wrapping a jacquard credential session.
pub struct TranquilClient {
    pub admin: Agent<MemoryCredentialSession>,
    http: reqwest::Client,
}

impl TranquilClient {
    /// Log in the admin from `ATPROTO_USER` / `ATPROTO_PASSWORD`.
    pub async fn login() -> Result<Self, Error> {
        let id = std::env::var("ATPROTO_USER")
            .map_err(|_| Error::Config("ATPROTO_USER not set".into()))?;
        let pw = std::env::var("ATPROTO_PASSWORD")
            .map_err(|_| Error::Config("ATPROTO_PASSWORD not set".into()))?;

        let http = reqwest::Client::builder()
            .build()
            .map_err(|e| Error::Other(format!("http client: {e}")))?;

        let session = MemoryCredentialSession::unauthenticated();

        let pds_uri = std::env::var("ATPROTO_PDS_HOST").ok().map(|host| {
            jacquard::deps::fluent_uri::Uri::parse(format!("https://{host}"))
                .expect("invalid ATPROTO_PDS_HOST")
        });

        session
            .login(
                CowStr::from(id.as_str()),
                CowStr::from(pw.as_str()),
                None::<CowStr<'static>>,
                None,
                None::<CowStr<'static>>,
                pds_uri,
            )
            .await
            .map_err(|e| Error::Auth(format!("admin login: {e}")))?;

        Ok(Self { admin: Agent::from(session), http })
    }

    /// Bearer token from the admin session.
    async fn bearer(&self) -> Result<String, Error> {
        match self.admin.inner().access_token().await {
            Some(jacquard::common::AuthorizationToken::Bearer(t)) => Ok(t.to_string()),
            _ => Err(Error::Auth("no admin token".into())),
        }
    }

    // ── Admin HTTP helpers ───────────────────────────────────────────

    async fn admin_post(&self, nsid: &str, body: serde_json::Value) -> Result<serde_json::Value, Error> {
        let url = format!("{}/xrpc/{nsid}", self.admin.endpoint().await);
        let resp = self.http.post(&url)
            .header("authorization", format!("Bearer {}", self.bearer().await?))
            .json(&body).send().await?;
        check(resp).await
    }

    async fn admin_get(&self, nsid: &str) -> Result<serde_json::Value, Error> {
        let url = format!("{}/xrpc/{nsid}", self.admin.endpoint().await);
        let resp = self.http.get(&url)
            .header("authorization", format!("Bearer {}", self.bearer().await?))
            .send().await?;
        check(resp).await
    }

    // ── Invite codes ─────────────────────────────────────────────────

    /// Get invite codes for the admin account.
    pub async fn get_invite_codes(&self) -> Result<Vec<serde_json::Value>, Error> {
        let v = self.admin_get("com.atproto.server.getAccountInviteCodes").await?;
        Ok(v["codes"].as_array().cloned().unwrap_or_default())
    }

    /// Create a new invite code (admin only).
    pub async fn create_invite_code(&self, use_count: i32) -> Result<String, Error> {
        let v = self.admin_post(
            "com.atproto.server.createInviteCode",
            serde_json::json!({"useCount": use_count}),
        ).await?;
        Ok(v["code"].as_str().unwrap_or_default().to_string())
    }

    // ── Create account ───────────────────────────────────────────────

    /// Create a new account with an invite code (no auth needed).
    ///
    /// Note: some PDSes require email verification before the account
    /// can log in. For those, use [`create_delegated`](Self::create_delegated)
    /// on Tranquil PDS which creates immediately-usable accounts.
    pub async fn create_account(
        &self,
        handle: &str,
        email: &str,
        password: &str,
        invite_code: &str,
    ) -> Result<(String, String), Error> {
        let url = format!("{}/xrpc/com.atproto.server.createAccount", self.admin.endpoint().await);
        let resp = self.http.post(&url).json(&serde_json::json!({
            "handle": handle,
            "email": email,
            "password": password,
            "inviteCode": invite_code,
        })).send().await?;
        let v: serde_json::Value = check(resp).await?;
        Ok((
            v["did"].as_str().unwrap_or_default().to_string(),
            v["handle"].as_str().unwrap_or_default().to_string(),
        ))
    }

    // ── Admin operations ─────────────────────────────────────────────

    /// Set the password on any account (admin only).
    pub async fn set_password(&self, did: &str, password: &str) -> Result<(), Error> {
        self.admin_post(
            "com.atproto.admin.updateAccountPassword",
            serde_json::json!({"did": did, "password": password}),
        ).await?;
        Ok(())
    }

    // ── Login as any account ─────────────────────────────────────────

    /// Log in as any account: set a random password, return a jacquard Agent.
    pub async fn login_as(&self, did: &str) -> Result<Agent<MemoryCredentialSession>, Error> {
        let pw = gen_password();
        self.set_password(did, &pw).await?;
        let session = MemoryCredentialSession::unauthenticated();
        session.login(
            CowStr::from(did), CowStr::from(pw.as_str()),
            None::<CowStr<'static>>, None, None::<CowStr<'static>>, None,
        ).await.map_err(|e| Error::Auth(format!("login as {did}: {e}")))?;
        Ok(Agent::from(session))
    }
}

fn gen_password() -> String {
    use rand::Rng;
    const C: &[u8] = b"abcdefghjkmnpqrstuvwxyz23456789";
    (0..24).map(|_| C[rand::thread_rng().gen_range(0..C.len())] as char).collect()
}

async fn check(resp: reqwest::Response) -> Result<serde_json::Value, Error> {
    let status = resp.status();
    let v: serde_json::Value = resp.json().await.map_err(Error::Http)?;
    if status.is_success() { Ok(v) } else { Err(Error::Api(status.as_u16(), v)) }
}