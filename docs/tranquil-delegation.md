# Tranquil PDS — Controlled Accounts (Delegation) API

> Based on the [Tranquil PDS](https://git.tangled.org/tranquil/tranquil-pds) source code (pulled 2026-06-01).

This document covers two related features:

1. **Controlled accounts** — the delegation system that lets a "main" account create and manage sub-accounts (called "delegated accounts" in the code), granting them scoped control.
2. **App passwords** — the standard AT Protocol mechanism for issuing scoped credentials, extended in Tranquil to carry delegation metadata.

## Key Architectural Insight

**A controller does NOT act under its own identity on behalf of a delegated account.**
Instead, the controller authenticates *as* the delegated account — the JWT's `sub` (subject)
claim is the **delegated account's DID**. The `act` (actor) claim carries the **controller's DID**
to record who is really performing the action. Scope checks are enforced against the intersection
of what the controller requested and what the delegation grant allows.

So the flow is:
1. The controller obtains a session/token whose `sub = delegated_did` and `act = controller_did`.
2. The controller calls standard AT Protocol endpoints (e.g. `com.atproto.repo.createRecord`)
   with `repo = delegated_did`.
3. The PDS verifies `repo == sub` (the token is bound to that repo), checks the scopes,
   and logs `controller_did` in the audit trail.

This is done via the **OAuth delegation auth endpoint** or by creating **app passwords scoped to the delegation**.

---

## 1. Concepts

| Term | Meaning |
|---|---|
| **Main account** | A regular PDS user (`account_type = 'personal'`) that can become a *controller* of delegated accounts. |
| **Delegated account** | A user whose `account_type = 'delegated'`. It always has a `controller_did` and a set of `granted_scopes` that limit what the controller can do on its behalf. |
| **Controller** | The DID of the main account that manages a delegated account. The controller can create app passwords, perform repo writes, upload blobs, etc. **within the scopes** granted on the delegation. |
| **Grant** | A row in `account_delegations` linking `(delegated_did, controller_did)` with a scope string and a `granted_by`. |
| **Scope** | A space-separated string of scope tokens (e.g. `"atproto"`, `"repo:*?action=create blob:*/*"`, `""` for read-only). Scopes are intersected with what the controller requests when creating app passwords or authorizing OAuth tokens. |

### Constraints

- A **controlled account cannot itself be a controller** of another account (enforced in `roles.rs`).
- An account that already controls other accounts **cannot accept additional controllers** (enforced in `roles.rs`).
- Delegations can be revoked, which cascades to revoke all app passwords and OAuth tokens issued under that delegation.

---

## 2. Creating a Main Account

Use the standard AT Protocol endpoint:

### `com.atproto.server.createAccount`

```http
POST /xrpc/com.atproto.server.createAccount
Content-Type: application/json

{
  "handle": "my-main-account.test",
  "email": "admin@example.com",
  "password": "SecurePass123!"
}
```

**Response** (200):
```json
{
  "did": "did:plc:abc123...",
  "handle": "my-main-account.test",
  "accessJwt": "...",
  "refreshJwt": "..."
}
```

This creates a regular `account_type = 'personal'` user. You'll use the returned JWT as the main auth token for managing controlled accounts.

---

## 3. Creating an App Password for the Main Account

Before you can use the delegation APIs via app passwords, you must first create an app password for the main account.

### `com.atproto.server.createAppPassword`

```http
POST /xrpc/com.atproto.server.createAppPassword
Authorization: Bearer <main-account-jwt>
Content-Type: application/json

{
  "name": "arbiter-client",
  "privileged": true
}
```

**Response** (200):
```json
{
  "name": "arbiter-client",
  "password": "abcd-efgh-ijkl-mnop",
  "createdAt": "2026-06-01T12:00:00Z",
  "privileged": true,
  "scopes": "transition:generic transition:chat.bsky"
}
```

> **Important:** The `password` field is only returned once. Save it securely.

This app password can now be used to log in:

```http
POST /xrpc/com.atproto.server.createSession
Content-Type: application/json

{
  "identifier": "my-main-account.test",
  "password": "abcd-efgh-ijkl-mnop"
}
```

The session JWT returned from this login can be used to call all the delegation endpoints below.

### App Password Fields

| Field | Type | Description |
|---|---|---|
| `name` | string | Human-readable label (must be unique per account). |
| `privileged` | bool | If `true`, scopes include chat.bsky; if `false`, only `transition:generic`. |
| `scopes` | string | Explicit scope string (overrides `privileged` flag if provided). |

### `com.atproto.server.listAppPasswords`

```http
GET /xrpc/com.atproto.server.listAppPasswords
Authorization: Bearer <jwt>
```

### `com.atproto.server.revokeAppPassword`

```http
POST /xrpc/com.atproto.server.revokeAppPassword
Authorization: Bearer <jwt>
Content-Type: application/json

{
  "name": "arbiter-client"
}
```

---

## 4. Delegation API Endpoints

All delegation endpoints are under the `/_delegation.*` prefix and are **non-standard** Tranquil extensions.

### 4.1 `_delegation.getScopePresets`

Returns predefined scope bundles.

```http
GET /xrpc/_delegation.getScopePresets
```

**Response** (200):
```json
{
  "presets": [
    {
      "name": "owner",
      "label": "Owner",
      "description": "Full control including delegation management",
      "scopes": "atproto"
    },
    {
      "name": "admin",
      "label": "Admin",
      "description": "Manage account settings, post content, upload media",
      "scopes": "atproto repo:* blob:*/* account:*?action=manage"
    },
    {
      "name": "editor",
      "label": "Editor",
      "description": "Post content and upload media",
      "scopes": "repo:*?action=create repo:*?action=update repo:*?action=delete blob:*/*"
    },
    {
      "name": "viewer",
      "label": "Viewer",
      "description": "Read-only access",
      "scopes": ""
    }
  ]
}
```

### 4.2 `_delegation.createDelegatedAccount`

Creates a new delegated (controlled) account. The authenticated user must be a **personal** (non-delegated) account.

```http
POST /xrpc/_delegation.createDelegatedAccount
Authorization: Bearer <main-account-or-app-password-jwt>
Content-Type: application/json

{
  "handle": "delegated-user.test",
  "email": "delegated@example.com",
  "controllerScopes": "atproto repo:* blob:*/*",
  "inviteCode": "optional-invite-code"
}
```

**Response** (200):
```json
{
  "did": "did:plc:def456...",
  "handle": "delegated-user.test"
}
```

| Field | Required | Description |
|---|---|---|
| `handle` | yes | The handle for the new delegated account. |
| `email` | no | Optional email for the delegated account. |
| `controllerScopes` | yes | Scope string limiting what the controller (you) can do through this delegation. See scopes below. |
| `inviteCode` | no | Required if the PDS has `invite_code_required` enabled. |

The created account:
- Is stored with `account_type = 'delegated'`.
- Gets a `controller_did` pointing to the creator.
- Has its signing key encrypted and stored, with genesis repo created.
- Does **not** get a password — it can only be accessed through the controller via delegated auth (OAuth flow) or by the controller creating app passwords for the delegated account.

> Once the delegated account is created, **the controller must authenticate as the delegated
> account** (not as themselves) to perform repo writes, blob uploads, etc. See section 8.

### 4.3 `_delegation.addController`

Adds another controller (by DID) to a delegated account. The authenticated user must have the `can_add_controllers` permission (i.e., they must not already control accounts).

```http
POST /xrpc/_delegation.addController
Authorization: Bearer <jwt>
Content-Type: application/json

{
  "controllerDid": "did:plc:xyz789...",
  "grantedScopes": "repo:*?action=create"
}
```

**Response** (200):
```json
{ "success": true }
```

The controller DID is resolved to check if it exists locally or on another PDS. Cross-PDS controllers are supported if they use HTTPS.

### 4.4 `_delegation.removeController`

Revokes a controller's access to a delegated account.

```http
POST /xrpc/_delegation.removeController
Authorization: Bearer <jwt>
Content-Type: application/json

{
  "controllerDid": "did:plc:xyz789..."
}
```

**Response** (200):
```json
{ "success": true }
```

This will also:
- Revoke all app passwords that were created by that controller.
- Revoke all OAuth tokens issued under that delegation.
- Log an audit entry.

### 4.5 `_delegation.updateControllerScopes`

Updates the scope grant for an existing controller on a delegated account.

```http
POST /xrpc/_delegation.updateControllerScopes
Authorization: Bearer <jwt>
Content-Type: application/json

{
  "controllerDid": "did:plc:xyz789...",
  "grantedScopes": "repo:* blob:*/*"
}
```

**Response** (200):
```json
{ "success": true }
```

### 4.6 `_delegation.listControllers`

Lists all active controllers for the authenticated user's account.

```http
GET /xrpc/_delegation.listControllers
Authorization: Bearer <jwt>
```

**Response** (200):
```json
{
  "controllers": [
    {
      "did": "did:plc:xyz789...",
      "handle": "controller-user.test",
      "grantedScopes": "repo:*?action=create",
      "grantedAt": "2026-06-01T12:00:00Z",
      "isActive": true,
      "isLocal": true
    }
  ]
}
```

### 4.7 `_delegation.listControlledAccounts`

Lists all accounts that the authenticated user controls.

```http
GET /xrpc/_delegation.listControlledAccounts
Authorization: Bearer <jwt>
```

**Response** (200):
```json
{
  "accounts": [
    {
      "did": "did:plc:def456...",
      "handle": "delegated-user.test",
      "grantedScopes": "atproto",
      "grantedAt": "2026-06-01T12:00:00Z"
    }
  ]
}
```

### 4.8 `_delegation.getAuditLog`

Returns the audit log for the authenticated account's delegations.

```http
GET /xrpc/_delegation.getAuditLog?limit=50&offset=0
Authorization: Bearer <jwt>
```

**Response** (200):
```json
{
  "entries": [
    {
      "id": "uuid...",
      "delegatedDid": "did:plc:def456...",
      "actorDid": "did:plc:abc123...",
      "controllerDid": "did:plc:abc123...",
      "actionType": "GrantCreated",
      "actionDetails": { "account_created": true, "granted_scopes": "atproto" },
      "createdAt": "2026-06-01T12:00:00Z"
    }
  ],
  "total": 1
}
```

Action types: `GrantCreated`, `GrantRevoked`, `ScopesModified`, `TokenIssued`, `RepoWrite`, `BlobUpload`, `AccountAction`.

### 4.9 `_delegation.resolveController`

Resolves a handle or DID to identity info, including whether the account is local.

```http
GET /xrpc/_delegation.resolveController?identifier=did:plc:xyz789...
```

or

```http
GET /xrpc/_delegation.resolveController?identifier=some-user.test
```

**Response** (200):
```json
{
  "did": "did:plc:xyz789...",
  "handle": "some-user.test",
  "pdsUrl": "https://pds.example.com",
  "isLocal": true
}
```

---

## 5. Scope Language

Scopes are space-separated tokens following the pattern:

```
<prefix>[:<detail>][?<param>=<value>&...]
```

| Prefix | Meaning |
|---|---|
| `atproto` | Full access (all scopes). |
| `repo:*` | Full repo access. |
| `repo:<collection>` | Access to a specific collection (e.g., `repo:app.bsky.feed.post`). |
| `blob:*/*` | All blob types. |
| `blob:<mime-type>/*` | Blobs matching a MIME type. |
| `rpc:*` | All RPC methods. |
| `rpc:<nsid>` | Specific RPC method. |
| `account:*` | All account attributes. |
| `account:<attr>?action=<action>` | Specific account action. |
| `identity:handle` | Update handle. |
| `transition:generic` | Standard transitions (scopes default for non-privileged). |
| `transition:chat.bsky` | Chat-related transitions. |
| `include:<name>` | Include a named scope bundle. |

Scope intersection is computed at request time: when a controller creates an app password for a delegated account, the **requested** scopes are intersected with the **granted** scopes. The result is what the app password actually gets.

---

## 6. Using App Passwords with Delegated Accounts

An app password created by the controller with appropriate scopes can be used to **log in directly as the delegated account** via `createSession`, because the session token will carry `act = controller_did` and `sub = delegated_did`. From that session, the controller can call standard AT Protocol endpoints (e.g., `com.atproto.repo.createRecord`) specifying `repo = delegated_did`.

To create an app password that acts as the delegated account, create it *on the delegated account* through the delegation OAuth flow (section 7), or — more practically — create the app password via `createAppPassword` while authenticated with a delegation-backed token (where `auth.controller_did` is populated). The scope intersection (below) ensures the app password covers only the granted actions.

When an app password is created by a controller (detected via `auth.controller_did`), the scopes are intersected:

```rust
// Pseudocode from app_password.rs
let requested = input.scopes.as_deref().unwrap_or("atproto");
let intersected = intersect_scopes(requested, granted_scopes.as_str());
```

The resulting scope is stored on the app password, and the `created_by_controller_did` field links back to the controller.

---

## 7. Authentication Flow for Delegated Accounts (OAuth + App Password)

There are two ways for a controller to authenticate *as* a delegated account:

### 7A. OAuth Delegation Flow

Tranquil implements an OAuth delegation flow that lets a controller authenticate on behalf of a delegated account:

1. Client initiates a standard OAuth PAR request with `login_hint = <delegated_did>`.
2. Client POSTs to `/_oauth/delegation/auth` with:
   ```json
   {
     "request_uri": "...",
     "delegated_did": "did:plc:def456...",
     "controller_did": "did:plc:abc123...",
     "password": "controller-password",
     "remember_device": false
   }
   ```
3. The PDS verifies the delegation grant exists, validates the controller's password, and binds both DIDs to the authorization request.
4. The controller is redirected to the consent screen.
5. On approval, tokens are issued with the `controller_did` embedded in the token data. Specifically, the OAuth access token JWT contains:
   - `sub` = the delegated account DID
   - `act` = `{ "sub": "<controller_did>" }` (RFC 8693)
   - `scope` = the intersection of requested and granted scopes
6. All subsequent API calls via that token carry `controller_did`, scope check, and audit logging.

### 7B. Session (App Password) Delegation

When creating an app password on the **delegated account** through a delegation-authenticated session, the resulting app password:

- Is stored with `created_by_controller_did = controller_did`
- Gets scopes intersected against the delegation grant
- When used via `createSession`, the resulting session JWT carries `act = controller_did`

---

## 8. How the Controller Makes API Calls on a Delegated Account

This is the key question: once you've created the delegation grant, how do you actually use it to write records, upload blobs, update the DID doc, etc.?

### Tokens carry the delegation in the `act` claim

The mechanism is **JWT delegation** (RFC 8693 OAuth `act` claim):

- The JWT's **`sub`** (subject) = the **delegated account's DID**
- The JWT's **`act`** (actor) = the **controller's DID**

This means the controller authenticates **as the delegated account**, and the PDS records that they are acting through a delegation grant.

### How it works in practice

#### Option A: OAuth Delegation Auth (the primary flow)

1. Client initiates a standard OAuth PAR request with `login_hint = <delegated_did>`
2. Client POSTs to `/_oauth/delegation/auth` with the controller's password
3. The PDS issues OAuth tokens where:
   - `sub = delegated_did`
   - `act = { sub: controller_did }`
   - `scope` = intersection of requested scopes and delegation grant scopes
4. The client uses these tokens to call standard AT Protocol endpoints

#### Option B: App Password on the Delegated Account

The controller can create an app password **on the delegated account** that embeds the `act` claim. You need to first get a delegation-backed session:

1. Use the OAuth delegation flow (Option A) to get a session token for the delegated account.
2. Call `com.atproto.server.createAppPassword` with that session — the resulting app password will have `created_by_controller_did` set and scopes intersected.
3. Use that app password via `createSession` to get a JWT with `act = controller_did`.
4. Make standard AT Protocol calls with `repo = delegated_did`.

#### Option C: Direct Session Login (for app passwords created on the main account)

If you create an app password on the **main account** (not the delegated one), that app password authenticates you as the main account. To work *on* a delegated account, you'd use that session to call `_delegation.createDelegatedAccount` and manage delegations — but for actual repo writes *as* the delegated account, you need Option A or B.

### What happens when writing records

When the controller calls e.g. `com.atproto.repo.createRecord`:

```http
POST /xrpc/com.atproto.repo.createRecord
Authorization: Bearer <jwt-with-sub=delegated_did&act=controller_did>
Content-Type: application/json

{
  "repo": "did:plc:delegated-user",
  "collection": "app.bsky.feed.post",
  "record": { ... }
}
```

1. **`prepare_repo_write`** checks `repo == sub` — this binds the write to the delegated account.
2. **`require_verified_or_delegated`** is called — delegated accounts skip email verification.
3. **Scope is checked** via `verify_repo_create` (validates against the JWT's `scope` claim, which was already intersected with the delegation grant).
4. **The commit is signed** with the delegated account's signing key (stored encrypted in the DB at creation time).
5. **`controller_did`** is passed to `finalize_commit`, which logs a `RepoWrite` audit event.

### What about DID doc updates?

DID PLC operations require signing with the account's PLC signing key. The delegated account has its own signing key, but the controller would need to either:

- Have been granted the `identity:*` scope (for handle updates via PLC)
- Use a service token obtained via `com.atproto.server.getServiceAuth` with `lxm` set to the specific operation

For full PLC operation control, the delegation scope must include the appropriate `identity:*` scope tokens.

### What about blob uploads?

Blob uploads work the same way: authenticate with the delegation-backed token, call `com.atproto.blob.uploadBlob`, and the `controller_did` is passed through for audit logging (see `crates/tranquil-api/src/repo/blob.rs`).

---

## 9. Practical Plan for Arbiter Usage

To give your arbiter access to create and manage multiple PDS repositories:

1. **Create one main account** on the Tranquil PDS via `com.atproto.server.createAccount`.
2. **Create an app password** for that main account via `com.atproto.server.createAppPassword` with `"privileged": true` (or explicit `"scopes": "atproto"`). Save the returned password.
3. **Use the app password** to authenticate API calls (via `com.atproto.server.createSession` or directly as a Basic-auth-like bearer).
4. **Create delegated accounts** via `_delegation.createDelegatedAccount` for each arbiter-managed repo. Use the preset scopes:
   - `"atproto"` for full control (owner preset)
   - `"repo:* blob:*/*"` for repo + blob access (admin preset)
   - `"repo:*?action=create repo:*?action=update repo:*?action=delete blob:*/*"` for editor access
5. **Authenticate as the delegated account** via the OAuth delegation flow (section 7A) — this gives you tokens where `sub = delegated_did` and `act = controller_did`.
6. **Perform repo operations** using those tokens with standard AT Protocol endpoints, specifying `repo = delegated_did`.

For each delegated account, the arbiter can:
- Create further scoped app passwords on it.
- Write records, upload blobs, manage settings within the scope grant.
- Revoke/rotate access by removing controllers or updating scopes.

The `_delegation.getAuditLog` endpoint provides a full trail of who did what on which delegated account.

---

## 10. Schema Reference

### `account_delegations` table

| Column | Type | Description |
|---|---|---|
| `id` | UUID PK | Auto-generated. |
| `delegated_did` | TEXT FK → users(did) | The controlled account. |
| `controller_did` | TEXT FK → users(did) | The controller account. |
| `granted_scopes` | TEXT | The scope string for this delegation. |
| `granted_at` | TIMESTAMPTZ | When the delegation was created. |
| `granted_by` | TEXT FK → users(did) | Who created the delegation. |
| `revoked_at` | TIMESTAMPTZ | Null if active. |
| `revoked_by` | TEXT FK → users(did) | Who revoked it. |

Unique index: `(delegated_did, controller_did)` where `revoked_at IS NULL`.

### `users` table additions

| Column | Type | Default |
|---|---|---|
| `account_type` | ENUM('personal', 'delegated') | `'personal'` |
| (existing columns remain — did, handle, email, etc.) | | |

### `app_passwords` addition

| Column | Type |
|---|---|
| `created_by_controller_did` | TEXT FK → users(did) (nullable) |

### `session_tokens` / `oauth_tokens` additions

`controller_did` column references the controller DID when the session/token was issued through the delegation flow.