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
        identity::{
            sign_plc_operation::SignPlcOperation, submit_plc_operation::SubmitPlcOperation,
        },
        server::{
            InviteCode, create_account::CreateAccount, create_invite_code::CreateInviteCode,
            describe_server::DescribeServer, get_account_invite_codes::GetAccountInviteCodes,
        },
    },
    client::{Agent, MemoryCredentialSession},
    deps::smol_str::SmolStr,
    types::{did::Did, string::Handle, value::to_data},
    xrpc::XrpcClient,
};

pub use jacquard;

/// Describes a service entry in a DID PLC operation (e.g. an AT Protocol PDS).
#[derive(Debug, Clone)]
pub struct PlcService {
    /// Service ID, e.g. `"#atproto_pds"`.
    pub id: String,
    /// Service type, e.g. `"AtprotoPersonalDataServer"`.
    pub r#type: String,
    /// Service endpoint URL.
    pub endpoint: String,
}

/// Admin client wrapping a jacquard credential session.
pub struct PdsAdminClient {
    pub admin: Agent<MemoryCredentialSession>,
}

impl PdsAdminClient {
    /// Login as with a PDS admin account.
    pub async fn login(id: &str, password: &str) -> anyhow::Result<Self> {
        let session =
            MemoryCredentialSession::authenticated(id.into(), password.into(), None, None)
                .await
                .context("Admin login error")?
                .0;

        Ok(Self {
            admin: Agent::from(session),
        })
    }

    pub async fn pds_endpoint(&self) -> String {
        self.admin.base_uri().await.to_string()
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
            .await?;
        Ok(())
    }

    /// Log in as any account: set a random password, return a jacquard Agent.
    pub async fn login_as(&self, did: &str) -> anyhow::Result<Agent<MemoryCredentialSession>> {
        let password = gen_password()?;
        self.set_password(did, &password)
            .await
            .context("setting password")?;
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

    // ── DID document service endpoints ──────────────────────────────

    /// Set the service endpoints in a user's DID document via PLC operation.
    ///
    /// This logs in as the user, requests a signed PLC operation from the
    /// user's PDS with the given services, and submits it to the PLC
    /// directory (`https://plc.directory`).
    ///
    /// The `services` slice replaces all service entries in the DID
    /// document. Pass every service the document should contain.
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use pds_admin_client::{PdsAdminClient, PlcService};
    /// # async fn example() -> anyhow::Result<()> {
    /// # let client = PdsAdminClient::login("admin", "pass").await?;
    /// client.set_service_endpoints(
    ///     "did:plc:abc123",
    ///     &[
    ///         PlcService {
    ///             id: "#atproto_pds".into(),
    ///             r#type: "AtprotoPersonalDataServer".into(),
    ///             endpoint: "https://pds.example.com".into(),
    ///         },
    ///     ],
    /// )
    /// .await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn set_service_endpoints(
        &self,
        did: &str,
        services: &[PlcService],
    ) -> anyhow::Result<()> {
        // 1. Log in as the target user so we can call signPlcOperation on their PDS.
        let agent = self.login_as(did).await?;

        // 2. Build the services map in the PLC operation format:
        //    { "#id": { "type": "...", "endpoint": "..." }, ... }
        let services_map = services
            .iter()
            .map(|s| {
                (
                    s.id.clone(),
                    serde_json::json!({
                        "type": s.r#type,
                        "endpoint": s.endpoint,
                    }),
                )
            })
            .collect::<serde_json::Map<String, serde_json::Value>>();

        let services_data = to_data(&serde_json::Value::Object(services_map))
            .context("failed to serialize services to AT Protocol Data value")?;

        // 3. Sign the PLC operation on the user's PDS.
        let response = agent
            .send(SignPlcOperation::<SmolStr> {
                also_known_as: None,
                rotation_keys: None,
                services: Some(services_data),
                token: None,
                verification_methods: None,
                extra_data: None,
            })
            .await
            .context("signPlcOperation failed")?;

        let output = response
            .into_output()
            .context("signPlcOperation returned error")?;
        let signed_operation = output.operation;

        agent
            .send(SubmitPlcOperation::<SmolStr> {
                operation: signed_operation,
                extra_data: None,
            })
            .await?;

        Ok(())
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
