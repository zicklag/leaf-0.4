//! Wrapper around jacquard for managing AT Protocol accounts via an admin.
//!
//! 1. [`TranquilClient::login`] — admin logs in.
//! 2. Admin creates invite codes, creates accounts via `createAccount`.
//! 3. [`login_as`](TranquilClient::login_as) — admin sets password then logs in
//!    as any account, returns a fresh jacquard [`Agent`].
//! 4. Use [`AgentSessionExt`] on the returned agent for CRUD.

use std::{i32, str::FromStr};

use anyhow::Context;
use jacquard::{
    api::com_atproto::{
        admin::update_account_password::UpdateAccountPassword,
        server::{
            InviteCode, create_account::CreateAccount, create_invite_code::CreateInviteCode,
            describe_server::DescribeServer, get_account_invite_codes::GetAccountInviteCodes,
        },
    },
    client::{Agent, MemoryCredentialSession},
    deps::smol_str::SmolStr,
    types::{did::Did, string::Handle},
    xrpc::XrpcClient,
};

pub use jacquard;

/// Admin client wrapping a jacquard credential session.
pub struct PdsAdminClient {
    pub admin: Agent<MemoryCredentialSession>,
}

impl PdsAdminClient {
    /// Login as with a PDS admin account.
    pub async fn login(id: &str, password: &str) -> anyhow::Result<Self> {
        let session =
            MemoryCredentialSession::authenticated(dbg!(id).into(), password.into(), None, None)
                .await
                .context("Admin login error")?
                .0;

        Ok(Self {
            admin: Agent::from(session),
        })
    }

    // ── Invite codes ─────────────────────────────────────────────────

    /// Get invite codes for the admin account.
    async fn get_invite_codes(&self) -> anyhow::Result<Vec<InviteCode<SmolStr>>> {
        let v = self
            .admin
            .send(GetAccountInviteCodes::new().build())
            .await?
            .into_output()?;
        Ok(v.codes)
    }

    /// Create a new invite code (admin only).
    async fn create_invite_code(&self, use_count: i32) -> anyhow::Result<SmolStr> {
        let v = self
            .admin
            .send(
                CreateInviteCode::<SmolStr>::new()
                    .use_count(use_count)
                    .build(),
            )
            .await?
            .into_output()?;
        Ok(v.code)
    }

    /// Create or get an existing invite code that can be used for creating a user account.
    async fn ensure_invite_code(&self) -> anyhow::Result<SmolStr> {
        let codes = self.get_invite_codes().await?;
        for code in codes {
            if code.disabled == false && code.available > 0 {
                return Ok(code.code);
            }
        }

        // If we didn't find any available codes, just create a new one
        let code = self.create_invite_code(i32::MAX).await?;

        Ok(code)
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
    ) -> anyhow::Result<Agent<MemoryCredentialSession>> {
        // Get the invite code
        let invite_code = self.ensure_invite_code().await?;
        let password = gen_password()?;

        // Create the account
        let resp = self
            .admin
            .send(
                CreateAccount::<SmolStr>::new()
                    .handle(Handle::new(handle.into())?)
                    .password(Some(password.clone().into()))
                    .email(Some("invalid@email.localhost".into()))
                    .invite_code(Some(invite_code.into()))
                    .build(),
            )
            .await?
            .into_output()?;

        // Create a login session
        let session =
            MemoryCredentialSession::authenticated(resp.did.into(), password.into(), None, None)
                .await?
                .0;

        // Return the agent
        Ok(Agent::from(session))
    }

    /// Set the password on any account (admin only).
    async fn set_password(&self, did: &str, password: &str) -> anyhow::Result<()> {
        self.admin
            .send(
                UpdateAccountPassword::<SmolStr>::new()
                    .did(Did::from_str(did)?)
                    .password(password)
                    .build(),
            )
            .await?
            .into_output()?;
        Ok(())
    }

    /// Log in as any account: set a random password, return a jacquard Agent.
    pub async fn login_as(&self, did: &str) -> anyhow::Result<Agent<MemoryCredentialSession>> {
        let password = gen_password()?;
        self.set_password(did, &password).await?;
        let session =
            MemoryCredentialSession::authenticated(did.into(), password.into(), None, None)
                .await?
                .0;
        Ok(Agent::from(session))
    }

    pub async fn handle_suffixes(&self) -> anyhow::Result<Vec<SmolStr>> {
        let info = self.admin.send(DescribeServer).await?.into_output()?;
        Ok(info.available_user_domains)
    }
}

fn gen_password() -> anyhow::Result<String> {
    Ok(passwords::PasswordGenerator {
        length: 24,
        numbers: true,
        lowercase_letters: true,
        uppercase_letters: true,
        symbols: true,
        spaces: false,
        exclude_similar_characters: false,
        strict: true,
    }
    .generate_one()
    .map_err(|_| anyhow::format_err!("Could not generate password"))?)
}
