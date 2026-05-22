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

access_level(obj) := obj.level if is_object(obj)
access_level(obj) := obj if not is_object(obj)

access_rank("ReadMemberList") := 0
access_rank("IsMember") := 1
access_rank("AddMembers") := 2
access_rank("RemoveMembers") := 3
access_rank("ConfigureSpace") := 4
access_rank("CreateSpaces") := 5
access_rank("RemoveSpace") := 6
access_rank("Owner") := 7

member_rank(member) := access_rank(access_level(member.access))

# ---------------------------------------------------------------------------
# Space data access helpers
# ---------------------------------------------------------------------------

space_members(space_key) := data.arbiter.spaces[space_key].members

space_config(space_key) := data.arbiter.spaces[space_key].config

# ---------------------------------------------------------------------------
# Resolved members (raw, may contain duplicates via different delegation paths)
# ---------------------------------------------------------------------------

# Direct member in the target space
resolved_members_raw contains member if {
	some entry in space_members(input.resource.spaceKey)
	entry.member.tag == "MemberDid"
	member := {"did": entry.member.value, "access": entry.access, "via": input.resource.spaceKey}
}

# Direct member inherited from $admin space
resolved_members_raw contains member if {
	input.resource.spaceKey != "$admin"
	some entry in space_members("$admin")
	entry.member.tag == "MemberDid"
	member := {"did": entry.member.value, "access": entry.access, "via": "$admin"}
}

# Delegated from a local space member in the target space
resolved_members_raw contains member if {
	some entry in space_members(input.resource.spaceKey)
	entry.member.tag == "MemberLocalSpace"
	child_key := entry.member.value
	some child_entry in space_members(child_key)
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
	some entry in space_members("$admin")
	entry.member.tag == "MemberLocalSpace"
	child_key := entry.member.value
	some child_entry in space_members(child_key)
	child_entry.member.tag == "MemberDid"
	member := {
		"did": child_entry.member.value,
		"access": min_access(child_entry.access, entry.access),
		"via": child_key,
	}
}

# Delegated from a resolved remote space in the target space
resolved_members_raw contains member if {
	some entry in space_members(input.resource.spaceKey)
	entry.member.tag == "MemberRemoteSpace"
	arbiter_did := entry.member.value.arbiterDid
	space_key := entry.member.value.spaceKey

	# This resolves asynchronously via __builtin_host_await
	some remote_entry in resolve_remote(arbiter_did, space_key)
	member := {
		"did": remote_entry.did,
		"access": min_access(remote_entry.access, entry.access),
		"via": concat("|", [arbiter_did, space_key]),
	}
}

# Delegated from a resolved remote space in the admin space
resolved_members_raw contains member if {
	input.resource.spaceKey != "$admin"
	some entry in space_members("$admin")
	entry.member.tag == "MemberRemoteSpace"
	arbiter_did := entry.member.value.arbiterDid
	space_key := entry.member.value.spaceKey
	some remote_entry in resolve_remote(arbiter_did, space_key)
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
	some higher in resolved_members_raw
	higher.did == member.did
	member_rank(higher) > member_rank(member)
}

higher_via(member) if {
	some tie in resolved_members_raw
	tie.did == member.did
	member_rank(tie) == member_rank(member)
	tie.via < member.via
}

resolved_members contains member if {
	some member in resolved_members_raw
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
		some member in resolved_members_raw
		member.did == input.requester
	}
	rank := max(ranks)
}

# ---------------------------------------------------------------------------
# Missing spaces: remote spaces that resolved to empty (no members)
# ---------------------------------------------------------------------------

missing_spaces contains entry if {
	some member in space_members(input.resource.spaceKey)
	member.member.tag == "MemberRemoteSpace"
	arbiter_did := member.member.value.arbiterDid
	space_key := member.member.value.spaceKey
	count(resolve_remote(arbiter_did, space_key)) == 0
	entry := {
		"space": member.member.value,
		"access": member.access,
	}
}

missing_spaces contains entry if {
	input.resource.spaceKey != "$admin"
	some member in space_members("$admin")
	member.member.tag == "MemberRemoteSpace"
	arbiter_did := member.member.value.arbiterDid
	space_key := member.member.value.spaceKey
	count(resolve_remote(arbiter_did, space_key)) == 0
	entry := {
		"space": member.member.value,
		"access": member.access,
	}
}

# ---------------------------------------------------------------------------
# Target member helpers (for set/remove)
# ---------------------------------------------------------------------------

target_exists_in_raw if {
	some entry in space_members(input.resource.spaceKey)
	entry.member == input.params.targetMember
}

raw_target_rank := rank if {
	some entry in space_members(input.resource.spaceKey)
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
	space_config(input.resource.spaceKey).publicMembers == true
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
	space_config(input.resource.spaceKey).publicRecords == true
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
	count(space_members("$admin")) == 1
}
