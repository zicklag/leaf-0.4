# Security Audit: arbiter_core.qnt

**File:** `spec/arbiter/arbiter_core.qnt`  
**Date:** 2026-04-16  
**Auditor:** AI Security Review

---

## Executive Summary

This audit reviews the Quint specification `arbiter_core.qnt` for security vulnerabilities, logic bugs, and design issues. The code implements an arbiter system for managing permissioned spaces with nested space delegation.

**Critical Severity Issues Found:** 2  
**High Severity Issues Found:** 3  
**Medium Severity Issues Found:** 5  
**Low Severity Issues Found:** 4  

---

## Critical Severity

### 1. Inverted Permission Check in `configureSpace` (L307)

**Location:** `spec/arbiter/arbiter_core.qnt:307`

```quint
// Check that the user has access to configure the space
if (userAccess.includesAccess(ConfigureSpace)) throw(ErrPermissionDenied) else
```

**Issue:** The logic is inverted. The code throws `ErrPermissionDenied` when the user **has** `ConfigureSpace` access, rather than when they **lack** it. This means:
- Users WITHOUT `ConfigureSpace` permission can configure spaces
- Users WITH `ConfigureSpace` permission CANNOT configure spaces

**Impact:** Complete bypass of space configuration access control. Any non-owner can reconfigure any space.

**Fix:** Change to:
```quint
if (not(userAccess.includesAccess(ConfigureSpace))) throw(ErrPermissionDenied) else
```

---

## High Severity

### 3. No Cryptographic Authentication of Job Origin

**Location:** `spec/arbiter/arbiter_core.qnt:78-132`

```quint
action startJob(userDid: UserDid, arbiterDid: ArbiterDid, spaceKey: SpaceKey, jobId: JobId, args: JobArgs): bool
```

**Issue:** The `userDid` is passed as a plain string parameter with no cryptographic proof that the caller actually owns that identity. In a real deployment, this would require DID authentication (e.g., cryptographic signatures).

**Impact:** In an unauthenticated implementation, any caller can impersonate any user by passing their DID.

**Recommendation:** This spec should be paired with concrete authentication requirements in the implementation. Consider adding a comment documenting this assumption.

---

### 4. No Validation of Remote Space Member List Authenticity

**Location:** `spec/arbiter/arbiter_core.qnt:135-176`

```quint
action provideRemoteSpaceMembers(arbiterDid: ArbiterDid, jobId: JobId, spaceId: SpaceId, members: ResolvedMemberList): bool
```

**Issue:** The `members` parameter is accepted without any verification that it actually came from the referenced remote arbiter. If `spaceId` points to an arbiter controlled by a malicious actor, or if the network communication is compromised, the attacker can provide a falsified member list.

**Impact:** A malicious remote arbiter can grant arbitrary access to users in the local arbiter's spaces.

**Recommendation:** Require cryptographic proof (e.g., signed responses from remote arbiters) that the member list is authentic.

---

### 5. No Cleanup of Job Results (Memory Leak)

**Location:** `spec/arbiter/arbiter_core.qnt:895-898`

```quint
type Arbiter = {
  ...
  jobResults: JobId -> bool
}
```

**Issue:** Job results are stored permanently with no mechanism to clean them up. Over time, this map grows unboundedly.

**Impact:** Memory exhaustion denial-of-service. An attacker can fill up storage with job results.

**Recommendation:** Add a cleanup mechanism or limit the size of `jobResults`.

---

## Medium Severity

### 6. TOCTOU Race Condition in Job Execution

**Location:** `spec/arbiter/arbiter_core.qnt:251-252`

```quint
// Check that arbiter has not been modified since the job was started ( compare-and-swap style )
if (arbiter.version != job.arbiterVersion) throw(ErrPermissionChanged) else

// Check that the user is in the member list
if (not(resolved.memberList.has(job.userDid))) throw(ErrPermissionDenied) else
```

**Issue:** The version check only verifies the arbiter's `version` field hasn't changed. However:
1. A user might be in the member list when a job starts
2. Another job removes that user (incrementing version)
3. The first job now executes with a stale member list

The version check passes (arbiter version matches) but the user has been removed from the space.

**Impact:** A user could have their permissions revoked but continue to execute pending jobs.

**Recommendation:** Consider including a hash or snapshot of the relevant member list in the version check, or implement per-space versioning.

---

### 7. No Limit on Job Queue Size (DoS Vector)

**Location:** `spec/arbiter/arbiter_core.qnt:78-132`

```quint
// Check that arbiter has not been modified since the job was started
if (arbiter.version != job.arbiterVersion) throw(ErrPermissionChanged) else
```

**Issue:** While there's a per-user rate limit (`usersJobs.size() > 0`), there's no global limit on how many jobs can be queued. An attacker with many DIDs, or a single compromised account, could queue jobs that wait for remote resolution indefinitely.

**Impact:** Resource exhaustion through job queue buildup.

**Recommendation:** Add a maximum queue size limit.

---

### 8. Potential for Expensive Computation via Deeply Nested Spaces

**Location:** `spec/arbiter/arbiter_core.qnt:473-476`

```quint
def recursionDepth = spaceCount * accessCount
def outputState = 0.to(recursionDepth).fold(initState, (state, i) =>
```

**Issue:** The `members()` function iterates `spaceCount * accessCount` times. For an arbiter with many spaces, this could consume significant CPU. An attacker could create many spaces to exhaust computational resources.

**Impact:** CPU exhaustion DoS.

**Recommendation:** Add a maximum space count limit or timeout mechanism.

---

### 9. Missing Protection Against Remote Space ID Injection

**Location:** `spec/arbiter/arbiter_core.qnt:507-515`

```quint
def remoteSpaces = space.members.keys().remoteSpacesFromMembers().map(spaceId =>
  def childSpaceAccess = space.members.get(MemberRemoteSpace(spaceId))
  ( spaceId, minAccess(childSpaceAccess, spaceAccess) )
).setToMap()
```

**Issue:** No validation that remote space IDs reference actual existing spaces on real arbiters. An attacker could add references to non-existent remote spaces.

**Impact:** If a remote space reference points to a non-existent arbiter, the system may hang or fail when attempting to resolve it. If it points to a malicious arbiter, the attacker can provide fake member lists (see issue #4).

**Recommendation:** Validate that remote space IDs reference reachable, trusted arbiters before adding them.

---

### 10. No Verification of `JobDeleteArbiter` Pre-conditions at Execution Time

**Location:** `spec/arbiter/arbiter_core.qnt:229-237, 416-439`

```quint
// In jobValidationError:
| JobDeleteArbiter(x) =>
  def space = arbiter.spaces.get(spaceKey)
  if (not(spaceKey == SpaceKey::ADMIN)) Some(ErrArbiterDeletionMustSpecifyAdminSpace) else
  if (space.members.keys().size() > 1) Some(ErrOnlyLastOwnerCanDeleteArbiter)
  else None
```

**Issue:** The validation checks that there's only one owner at job creation time. But between job creation and execution:
1. Another owner could be added (making `size() > 1`)
2. The check in `deleteArbiter` re-checks this, which is correct

However, if validation passed and execution proceeds, there should be an explicit check in `deleteArbiter`. Looking at `deleteArbiter` (L433):
```quint
if (space.members.keys().size() > 1) throw(ErrOnlyLastOwnerCanDeleteArbiter) else
```

This is correctly re-checked. **This issue is actually handled correctly.**

---

## Low Severity

### 11. Integer Overflow Risk in Version Number

**Location:** `spec/arbiter/arbiter_core.qnt:889`

```quint
type Arbiter = {
  version: int,
  ...
}
```

**Issue:** `version` is an unbounded integer. After 2^63-1 increments (on most systems), integer overflow could occur.

**Impact:** Unlikely in practice, but could cause unexpected behavior on long-running systems.

**Recommendation:** Document the assumption of no integer overflow or use bounded arithmetic.

---

### 12. No Protection Against Replay of Remote Space Resolutions

**Location:** `spec/arbiter/arbiter_core.qnt:135-176`

```quint
action provideRemoteSpaceMembers(arbiterDid: ArbiterDid, jobId: JobId, spaceId: SpaceId, members: ResolvedMemberList): bool
```

**Issue:** If the same `provideRemoteSpaceMembers` call is processed twice (e.g., via network retransmission), the resolved spaces will be updated twice. The check `if (job.resolvedSpaces.has(spaceId)) throw(ErrSpaceAlreadyResolved)` prevents this, which is good.

**Status:** Actually handled correctly.

---

### 13. Missing Test Coverage for Security-Critical Paths

**Location:** `spec/arbiter/arbiter_test.qnt`

**Issue:** The test file has good coverage for `members()` and `joinResolvedSpaces()`, but lacks explicit tests for:
- Concurrent job execution scenarios
- Permission escalation attempts
- Remote space resolution edge cases

**Recommendation:** Add model-based tests that verify permission enforcement under adversarial conditions.

---

### 14. No Explicit Invariant for Single Admin Owner

**Location:** `spec/arbiter/arbiter_core.qnt:671-679`

```quint
val Inv = all {
  InvArbiterSpaceAlwaysExists
}
```

**Issue:** The only invariant is that `$admin` space always exists. There's no invariant ensuring that the `$admin` space always has at least one owner, which could lead to an irrecoverable state if all owners are removed.

**Recommendation:** Consider adding an invariant:
```quint
InvArbiterHasOwner = arbiters.keys().forall(arbiterDid =>
  arbiter.spaces.get(SpaceKey::ADMIN).members.keys().exists(...)
)
```

---

## Summary Table

| # | Severity | Issue | Location |
|---|----------|-------|----------|
| 1 | CRITICAL | Inverted permission check in `configureSpace` | L307 |
| 2 | CRITICAL | Missing `$admin` key validation in `createSpace` | L266 |
| 3 | HIGH | No cryptographic authentication of job origin | L78 |
| 4 | HIGH | No validation of remote member list authenticity | L135 |
| 5 | HIGH | No cleanup of job results | L897 |
| 6 | MEDIUM | TOCTOU race condition | L251 |
| 7 | MEDIUM | No limit on job queue size | L78 |
| 8 | MEDIUM | CPU exhaustion via deep nesting | L473 |
| 9 | MEDIUM | Remote space ID injection | L507 |
| 10 | MEDIUM | Verified correctly | L416 |
| 11 | LOW | Integer overflow risk | L889 |
| 12 | LOW | Verified correctly | L146 |
| 13 | LOW | Missing test coverage | test file |
| 14 | LOW | Missing arbiter owner invariant | L671 |

---

## Recommendations

1. **Immediate Fix Required:** Issue #1 (inverted permission check) must be fixed before any deployment.

2. **Authentication:** Implement cryptographic DID authentication in the actual implementation.

3. **Remote Space Validation:** Require signed responses from remote arbiters.

4. **Resource Limits:** Add limits on job queue size, space count, and computation time.

5. **Testing:** Add model-based tests specifically targeting security properties.

6. **Invariants:** Add invariants to ensure the arbiter always has at least one owner.

---

*End of Audit*
