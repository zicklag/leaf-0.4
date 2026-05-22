# Default access-level authorization policy for the Muni Town Arbiter.
#
# This policy uses:
#   - data.arbiter.spaces[space_key]  — local arbiter state (frozen snapshot)
#   - resolve_remote(arbiter_did, space_key)  — async host builtin for remote spaces
#
# The VM may suspend on resolve_remote calls. The host fetches the data and
# resumes the VM transparently. The policy does not distinguish between local
# and remote resolution at the rule level.

package arbiter

import rego.v1

# ---------------------------------------------------------------------------
# Access level helpers
# ---------------------------------------------------------------------------

access_level(obj) := level if {
    is_object(obj)
    level := obj.level
}

access_level(obj) := level if {
    not is_object(obj)
    level := obj
}

access_rank(level) := 0 if level == "ReadMemberList"
access_rank(level) := 1 if level == "IsMember"
access_rank(level) := 2 if level == "AddMembers"
access_rank(level) := 3 if level == "RemoveMembers"
access_rank(level) := 4 if level == "ConfigureSpace"
access_rank(level) := 5 if level == "CreateSpaces"
access_rank(level) := 6 if level == "RemoveSpace"
access_rank(level) := 7 if level == "Owner"

member_rank(member) := rank if {
    rank := access_rank(access_level(member.access))
}

# ---------------------------------------------------------------------------
# Space data access helpers
# ---------------------------------------------------------------------------

# Get members of a space from the frozen data snapshot.
space_members(space_key) := members if {
    members := data.arbiter.spaces[space_key].members
}

# Get config of a space from the frozen data snapshot.
space_config(space_key) := config if {
    config := data.arbiter.spaces[space_key].config
}

# ---------------------------------------------------------------------------
# Resolved members (raw, may contain duplicates via different delegation paths)
# ---------------------------------------------------------------------------

# Direct member in the target space
resolved_members_raw contains member if {
    raw := space_members(input.resource.spaceKey)
    entry := raw[_]
    entry.member.tag == "MemberDid"
    member := {"did": entry.member.value, "access": entry.access, "via": input.resource.spaceKey}
}

# Direct member inherited from $admin space
resolved_members_raw contains member if {
    input.resource.spaceKey != "$admin"
    raw := space_members("$admin")
    entry := raw[_]
    entry.member.tag == "MemberDid"
    member := {"did": entry.member.value, "access": entry.access, "via": "$admin"}
}

# Delegated from a local space member in the target space
resolved_members_raw contains member if {
    raw := space_members(input.resource.spaceKey)
    entry := raw[_]
    entry.member.tag == "MemberLocalSpace"
    child_key := entry.member.value
    child_raw := space_members(child_key)
    child_entry := child_raw[_]
    child_entry.member.tag == "MemberDid"
    member := {
        "did": child_entry.member.value,
        "access": min_access(child_entry.access, entry.access),
        "via": child_key,
    }
}

# Delegated from a local space member in the admin space
resolved_members_raw contains member if {
    input.resource.spaceKey != "$admin"
    raw := space_members("$admin")
    entry := raw[_]
    entry.member.tag == "MemberLocalSpace"
    child_key := entry.member.value
    child_raw := space_members(child_key)
    child_entry := child_raw[_]
    child_entry.member.tag == "MemberDid"
    member := {
        "did": child_entry.member.value,
        "access": min_access(child_entry.access, entry.access),
        "via": child_key,
    }
}

# Delegated from a resolved remote space in the target space
resolved_members_raw contains member if {
    raw := space_members(input.resource.spaceKey)
    entry := raw[_]
    entry.member.tag == "MemberRemoteSpace"
    arbiter_did := entry.member.value.arbiterDid
    space_key := entry.member.value.spaceKey
    # This resolves asynchronously via __builtin_host_await
    resolved := resolve_remote(arbiter_did, space_key)
    remote_entry := resolved[_]
    member := {
        "did": remote_entry.did,
        "access": min_access(remote_entry.access, entry.access),
        "via": concat("|", [arbiter_did, space_key]),
    }
}

# Delegated from a resolved remote space in the admin space
resolved_members_raw contains member if {
    input.resource.spaceKey != "$admin"
    raw := space_members("$admin")
    entry := raw[_]
    entry.member.tag == "MemberRemoteSpace"
    arbiter_did := entry.member.value.arbiterDid
    space_key := entry.member.value.spaceKey
    resolved := resolve_remote(arbiter_did, space_key)
    remote_entry := resolved[_]
    member := {
        "did": remote_entry.did,
        "access": min_access(remote_entry.access, entry.access),
        "via": concat("|", [arbiter_did, space_key]),
    }
}

# ---------------------------------------------------------------------------
# Deduplicated: each DID appears once with their highest access
# ---------------------------------------------------------------------------

higher_exists(member) if {
    higher := resolved_members_raw[_]
    higher.did == member.did
    member_rank(higher) > member_rank(member)
}

higher_via(member) if {
    tie := resolved_members_raw[_]
    tie.did == member.did
    member_rank(tie) == member_rank(member)
    tie.via < member.via
}

resolved_members contains member if {
    member := resolved_members_raw[_]
    not higher_exists(member)
    not higher_via(member)
}

# ---------------------------------------------------------------------------
# min_access
# ---------------------------------------------------------------------------

min_access(a, b) := a if {
    member_rank({"access": a}) <= member_rank({"access": b})
}

min_access(a, b) := b if {
    member_rank({"access": b}) < member_rank({"access": a})
}

# ---------------------------------------------------------------------------
# Requester info
# ---------------------------------------------------------------------------

requester_rank := rank if {
    ranks := {member_rank(member) |
        member := resolved_members_raw[_]
        member.did == input.requester
    }
    rank := max(ranks)
}

# ---------------------------------------------------------------------------
# Missing spaces: remote spaces that resolved to empty (no members)
# ---------------------------------------------------------------------------

missing_spaces contains entry if {
    raw := space_members(input.resource.spaceKey)
    member := raw[_]
    member.member.tag == "MemberRemoteSpace"
    arbiter_did := member.member.value.arbiterDid
    space_key := member.member.value.spaceKey
    resolved := resolve_remote(arbiter_did, space_key)
    count(resolved) == 0
    entry := {
        "space": member.member.value,
        "access": member.access,
    }
}

missing_spaces contains entry if {
    input.resource.spaceKey != "$admin"
    raw := space_members("$admin")
    member := raw[_]
    member.member.tag == "MemberRemoteSpace"
    arbiter_did := member.member.value.arbiterDid
    space_key := member.member.value.spaceKey
    resolved := resolve_remote(arbiter_did, space_key)
    count(resolved) == 0
    entry := {
        "space": member.member.value,
        "access": member.access,
    }
}

# ---------------------------------------------------------------------------
# Target member helpers (for set/remove)
# ---------------------------------------------------------------------------

target_exists_in_raw if {
    raw := space_members(input.resource.spaceKey)
    entry := raw[_]
    entry.member == input.params.targetMember
}

raw_target_rank := rank if {
    raw := space_members(input.resource.spaceKey)
    entry := raw[_]
    entry.member == input.params.targetMember
    rank := member_rank(entry)
}

# ---------------------------------------------------------------------------
# Authorization rules
# ---------------------------------------------------------------------------

default allow := false

# --- Reads ---

# Public member list: anyone can read
allow if {
    input.action in {"resolveSpaceMembers", "getSpaceMembers"}
    config := space_config(input.resource.spaceKey)
    config.publicMembers == true
}

# Non-public: need ReadMemberList
allow if {
    input.action in {"resolveSpaceMembers", "getSpaceMembers"}
    requester_rank >= access_rank("ReadMemberList")
}

allow if {
    input.action in {"getArbiterConfig", "listSpaces"}
    requester_rank >= access_rank("ReadMemberList")
}

# Public records: anyone can read space config
allow if {
    input.action == "getSpaceConfig"
    config := space_config(input.resource.spaceKey)
    config.publicRecords == true
}

allow if {
    input.action == "getSpaceConfig"
    requester_rank >= access_rank("ReadMemberList")
}

# --- Writes ---

allow if {
    input.action == "createSpace"
    requester_rank >= access_rank("CreateSpaces")
}

allow if {
    input.action == "setSpaceConfig"
    requester_rank >= access_rank("ConfigureSpace")
}

allow if {
    input.action == "deleteSpace"
    requester_rank >= access_rank("RemoveSpace")
}

allow if {
    input.action == "setArbiterConfig"
    requester_rank >= access_rank("Owner")
}

allow if {
    input.action == "setSpaceMemberAccess"
    requester_rank >= access_rank("AddMembers")
    access_rank(input.params.targetAccess.level) <= requester_rank
    not target_exists_in_raw
}

allow if {
    input.action == "setSpaceMemberAccess"
    requester_rank >= access_rank("AddMembers")
    access_rank(input.params.targetAccess.level) <= requester_rank
    target_exists_in_raw
    requester_rank >= access_rank("RemoveMembers")
    raw_target_rank <= requester_rank
}

allow if {
    input.action == "removeSpaceMember"
    requester_rank >= access_rank("RemoveMembers")
    target_exists_in_raw
    raw_target_rank <= requester_rank
}

allow if {
    input.action == "deleteArbiter"
    requester_rank >= access_rank("Owner")
    admin_members := space_members("$admin")
    count(admin_members) == 1
}
