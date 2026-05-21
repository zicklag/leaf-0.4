# Default access-level authorization policy for the Muni Town Arbiter.

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
# Resolved members (raw, may contain duplicates via different delegation paths)
# ---------------------------------------------------------------------------

# Direct member in the target space
resolved_members_raw contains member if {
    raw := arbiter.get_space_members(input.resource.spaceKey)
    entry := raw[_]
    entry.member.tag == "MemberDid"
    member := {"did": entry.member.value, "access": entry.access, "via": input.resource.spaceKey}
}

# Direct member inherited from $admin space
resolved_members_raw contains member if {
    input.resource.spaceKey != "$admin"
    raw := arbiter.get_space_members("$admin")
    entry := raw[_]
    entry.member.tag == "MemberDid"
    member := {"did": entry.member.value, "access": entry.access, "via": "$admin"}
}

# Delegated from a local space member in the target space
resolved_members_raw contains member if {
    raw := arbiter.get_space_members(input.resource.spaceKey)
    entry := raw[_]
    entry.member.tag == "MemberLocalSpace"
    child_key := entry.member.value
    child_raw := arbiter.get_space_members(child_key)
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
    raw := arbiter.get_space_members("$admin")
    entry := raw[_]
    entry.member.tag == "MemberLocalSpace"
    child_key := entry.member.value
    child_raw := arbiter.get_space_members(child_key)
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
    raw := arbiter.get_space_members(input.resource.spaceKey)
    entry := raw[_]
    entry.member.tag == "MemberRemoteSpace"
    remote_id := concat("|", [entry.member.value.arbiterDid, entry.member.value.spaceKey])
    resolved := input.resolved_remotes[remote_id]
    remote_entry := resolved[_]
    member := {
        "did": remote_entry.did,
        "access": min_access(remote_entry.access, entry.access),
        "via": remote_id,
    }
}

# Delegated from a resolved remote space in the admin space
resolved_members_raw contains member if {
    input.resource.spaceKey != "$admin"
    raw := arbiter.get_space_members("$admin")
    entry := raw[_]
    entry.member.tag == "MemberRemoteSpace"
    remote_id := concat("|", [entry.member.value.arbiterDid, entry.member.value.spaceKey])
    resolved := input.resolved_remotes[remote_id]
    remote_entry := resolved[_]
    member := {
        "did": remote_entry.did,
        "access": min_access(remote_entry.access, entry.access),
        "via": remote_id,
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
# Remote spaces needing async resolution
# ---------------------------------------------------------------------------

needs_resolution contains entry if {
    raw := arbiter.get_space_members(input.resource.spaceKey)
    member := raw[_]
    member.member.tag == "MemberRemoteSpace"
    remote_id := concat("|", [member.member.value.arbiterDid, member.member.value.spaceKey])
    not input.resolved_remotes[remote_id]
    entry := {"remoteArbiterDid": member.member.value.arbiterDid, "spaceKey": member.member.value.spaceKey}
}

needs_resolution contains entry if {
    input.resource.spaceKey != "$admin"
    raw := arbiter.get_space_members("$admin")
    member := raw[_]
    member.member.tag == "MemberRemoteSpace"
    remote_id := concat("|", [member.member.value.arbiterDid, member.member.value.spaceKey])
    not input.resolved_remotes[remote_id]
    entry := {"remoteArbiterDid": member.member.value.arbiterDid, "spaceKey": member.member.value.spaceKey}
}

# ---------------------------------------------------------------------------
# Missing spaces: remote spaces that were expected but returned empty
# ---------------------------------------------------------------------------

missing_spaces contains entry if {
    raw := arbiter.get_space_members(input.resource.spaceKey)
    member := raw[_]
    member.member.tag == "MemberRemoteSpace"
    remote_id := concat("|", [member.member.value.arbiterDid, member.member.value.spaceKey])
    remote_data := input.resolved_remotes[remote_id]
    count(remote_data) == 0
    entry := {
        "space": member.member.value,
        "access": member.access,
    }
}

missing_spaces contains entry if {
    input.resource.spaceKey != "$admin"
    raw := arbiter.get_space_members("$admin")
    member := raw[_]
    member.member.tag == "MemberRemoteSpace"
    remote_id := concat("|", [member.member.value.arbiterDid, member.member.value.spaceKey])
    remote_data := input.resolved_remotes[remote_id]
    count(remote_data) == 0
    entry := {
        "space": member.member.value,
        "access": member.access,
    }
}

# ---------------------------------------------------------------------------
# Target member helpers (for set/remove)
# ---------------------------------------------------------------------------

target_exists_in_raw if {
    raw := arbiter.get_space_members(input.resource.spaceKey)
    entry := raw[_]
    entry.member == input.params.targetMember
}

raw_target_rank := rank if {
    raw := arbiter.get_space_members(input.resource.spaceKey)
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
    config := arbiter.get_space_config(input.resource.spaceKey)
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
    config := arbiter.get_space_config(input.resource.spaceKey)
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
    admin_members := arbiter.get_space_members("$admin")
    count(admin_members) == 1
}
