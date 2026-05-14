# Arbiter Simulator — Next Steps

**Last updated: 2026-05-13**

## Current State

- **`crates/arbiter-wasm/`**: Wasm crate with tsify (`js` feature). All view types and Message types auto-generate `.d.ts` via wasm-bindgen's `typescript_custom_section`.
- **`arbiter-simulator/`**: Svelte 5 SPA, builds with Vite + `vite-plugin-wasm`. Uses typed imports from `arbiter-wasm`.

## Remaining Type Issues

### 1. EffectView inner fields are snake_case (tsify limitation)
tsify doesn't apply `rename_all = "camelCase"` to enum variant fields:

```typescript
// Generated (actual):
{ effectType: "respond"; req_id: number; ok: boolean; member_list: ... }

// Desired:
{ effectType: "respond"; reqId: number; ok: boolean; memberList: ... }
```

**Options:**
- Accept snake_case (code already matches this)
- Rename Rust fields to camelCase in the EffectView enum
- Use `#[serde(rename = "reqId")]` on each field

### 2. `ResolvedMemberList` uses `Map<K, V>`
```typescript
export interface ResolvedMemberList {
    memberList: Map<string, Access>;
    missingSpaces: Map<SpaceId, Access>;
}
```

The serializer creates JS `Map` objects by default. When constructing `ReplyResolvedMembers` in the JS simulator, we need to use `new Map()` instead of plain objects.

**Options:**
- Use `new Map()` in JS code
- Add `#[tsify(hashmap_as_object)]` to make them plain objects in TS

### 3. `configureSpace` uses snake_case fields
```typescript
{ type: "configureSpace"; public_records: boolean; public_members: boolean }
```

Already fixed in ActionPanel.svelte.

### 4. Cleanup: remove `accessTag` from utils.ts
`Access` is now a plain string union — the `accessTag` helper is no longer needed.

## Features to Test

1. **Create arbiter** — should work with typed Message input
2. **Create space** — same
3. **Add member** — Access is now a string, Member is tagged format
4. **Delegation** — auto-resolution via FetchMembers → ReplyResolvedMembers
5. **Fetch members** — returns EffectView with snake_case fields
6. **Permission denial** — Bob shouldn't be able to delete Alice's arbiter
7. **Delete arbiter** — only as sole owner
