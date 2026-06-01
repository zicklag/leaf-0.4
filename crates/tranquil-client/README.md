# tranquil-client

Minimal Rust client for managing AT Protocol accounts on a Tranquil PDS.

## What it does

1. **Admin logs in** — authenticates with password or app-password
2. **Create delegated accounts** — creates controlled sub-accounts
3. **Set passwords** — admin sets password on any account
4. **Login as anyone** — standard password auth
5. **CRUD records** — create, read, list, put, delete

No OAuth, no DPoP, no local servers. Just Bearer token auth.

## Usage

```rust
use tranquil_client::{ClientConfig, TranquilClient};

let config = ClientConfig::from_env()?;
let http = reqwest::Client::builder().build()?;
let mut client = TranquilClient::new(config, http);
client.login().await?;                              // admin login

let acct = client.create_delegated("my-user", "atproto").await?;
let did = acct["did"].as_str().unwrap();

let pw = "some-secure-password";
client.set_password(did, pw).await?;                 // admin only

let session = client.login_as(&acct["handle"], pw).await?;
client.create_record(did, "app.bsky.feed.post",
    serde_json::json!({"$type":"app.bsky.feed.post","text":"hi"}),
    &session.access_token).await?;
```

## Env

```
ATPROTO_USER=<handle or DID>
ATPROTO_PASSWORD=<password>
```

Proxy: respects `http_proxy` / `https_proxy` env vars (reqwest default).
TLS: rustls with system cert store (`SSL_CERT_FILE` respected).

## Test

```sh
cargo test -p tranquil-client --test tranquil_integration -- --test-threads=1 --nocapture
```