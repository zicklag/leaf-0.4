# Default access-level authorization policy for the Muni Town Arbiter.
#
# This policy evaluates whether a given XRPC operation is allowed. It
# receives:
#
#   data.arbiter             — the arbiter's full state (config + spaces)
#   data.arbiter.spaces[key] — a space with its config and members
#   input.caller.did         — the requester's DID
#   input.caller.access      — the requester's pre-computed access level
#   input.operation.nsid     — the XRPC method NSID
#   input.operation.params   — the method parameters
#
# The policy can request additional data via two host built-ins:
#   xrpc_local(path, params)   — query the local arbiter
#   xrpc_remote(did, path, params) — query a remote arbiter
#
# XRPC queries from the policy are ALWAYS read-only (never procedures).

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

space_members(space_key) := members if {
	data.arbiter.spaces[space_key]
	members := data.arbiter.spaces[space_key].members
}

space_members(space_key) := [] if {
	not data.arbiter.spaces[space_key]
}

space_config(space_key) := config if {
	data.arbiter.spaces[space_key]
	config := data.arbiter.spaces[space_key].config
}

space_config(space_key) := {} if {
	not data.arbiter.spaces[space_key]
}

# ---------------------------------------------------------------------------
# Resolved members via local/remote delegation
# ---------------------------------------------------------------------------

# Direct member in the target space
resolved_members_raw contains member if {
	some entry in space_members(input.operation.params.spaceKey)
	member := {"did": entry.did, "access": entry.access, "via": input.operation.params.spaceKey}
}

# Direct member inherited from $admin space
resolved_members_raw contains member if {
	input.operation.params.spaceKey != "$admin"
	some entry in space_members("$admin")
	member := {"did": entry.did, "access": entry.access, "via": "$admin"}
}

# Delegated from a local space: the member DID is "space:<key>"
resolved_members_raw contains member if {
	some entry in space_members(input.operation.params.spaceKey)
	startswith(entry.did, "space:")
	child_key := trim_prefix(entry.did, "space:")
	some child_entry in space_members(child_key)
	member := {
		"did": child_entry.did,
		"access": min_access(child_entry.access, entry.access),
		"via": child_key,
	}
}

# Delegated from a local space in the admin space
resolved_members_raw contains member if {
	input.operation.params.spaceKey != "$admin"
	some entry in space_members("$admin")
	startswith(entry.did, "space:")
	child_key := trim_prefix(entry.did, "space:")
	some child_entry in space_members(child_key)
	member := {
		"did": child_entry.did,
		"access": min_access(child_entry.access, entry.access),
		"via": child_key,
	}
}

# Delegated from a remote space: the member DID is "<arbiterDid>|<spaceKey>"
resolved_members_raw contains member if {
	some entry in space_members(input.operation.params.spaceKey)
	contains(entry.did, "|")
	parts := split(entry.did, "|")
	arbiter_did := parts[0]
	space_key := parts[1]

	# This resolves asynchronously via xrpc_remote → __builtin_host_await
	some remote_entry in xrpc_remote(arbiter_did, "town.muni.arbiter.resolveSpaceMembers", {"spaceKey": space_key})
	member := {
		"did": remote_entry.did,
		"access": min_access(remote_entry.access, entry.access),
		"via": concat("|", [arbiter_did, space_key]),
	}
}

# Delegated from a remote space in the admin space
resolved_members_raw contains member if {
	input.operation.params.spaceKey != "$admin"
	some entry in space_members("$admin")
	contains(entry.did, "|")
	parts := split(entry.did, "|")
	arbiter_did := parts[0]
	space_key := parts[1]
	some remote_entry in xrpc_remote(arbiter_did, "town.muni.arbiter.resolveSpaceMembers", {"spaceKey": space_key})
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

resolved_members contains member if {
	some member in resolved_members_raw
	not higher_exists(member)
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
		member.did == input.caller.did
	}
	rank := max(ranks)
}

# ---------------------------------------------------------------------------
# Missing spaces: remote references that resolved to empty
# ---------------------------------------------------------------------------

missing_spaces contains ms if {
	some entry in space_members(input.operation.params.spaceKey)
	contains(entry.did, "|")
	parts := split(entry.did, "|")
	arbiter_did := parts[0]
	space_key := parts[1]
	count(xrpc_remote(arbiter_did, "town.muni.arbiter.resolveSpaceMembers", {"spaceKey": space_key})) == 0
	ms := {"arbiterDid": arbiter_did, "spaceKey": space_key}
}

# ---------------------------------------------------------------------------
# Target member helpers (for set/remove)
# ---------------------------------------------------------------------------

target_exists_in_raw if {
	some entry in space_members(input.operation.params.spaceKey)
	entry.did == input.operation.params.memberDid
}

raw_target_rank := rank if {
	some entry in space_members(input.operation.params.spaceKey)
	entry.did == input.operation.params.memberDid
	rank := member_rank(entry)
}

resolve_result := {"members": resolved_members, "missingSpaces": missing_spaces}

# ---------------------------------------------------------------------------
# Authorization rules
# ---------------------------------------------------------------------------

default allow := false

# --- Reads ---

# Public member list: anyone can read
allow if {
	input.operation.nsid in {"town.muni.arbiter.resolveSpaceMembers", "town.muni.arbiter.getSpaceMembers"}
	space_config(input.operation.params.spaceKey).publicMembers == true
}

allow if {
	input.operation.nsid in {"town.muni.arbiter.resolveSpaceMembers", "town.muni.arbiter.getSpaceMembers"}
	requester_rank >= access_rank("ReadMemberList")
}

allow if {
	input.operation.nsid in {"town.muni.arbiter.getArbiterConfig", "town.muni.arbiter.listSpaces"}
	requester_rank >= access_rank("ReadMemberList")
}

# Public records: anyone can read space config
allow if {
	input.operation.nsid == "town.muni.arbiter.getSpaceConfig"
	space_config(input.operation.params.spaceKey).publicRecords == true
}

allow if {
	input.operation.nsid == "town.muni.arbiter.getSpaceConfig"
	requester_rank >= access_rank("ReadMemberList")
}

# --- Writes ---

allow if {
	input.operation.nsid == "town.muni.arbiter.createSpace"
	requester_rank >= access_rank("CreateSpaces")
}

allow if {
	input.operation.nsid == "town.muni.arbiter.setSpaceConfig"
	requester_rank >= access_rank("ConfigureSpace")
}

allow if {
	input.operation.nsid == "town.muni.arbiter.deleteSpace"
	requester_rank >= access_rank("RemoveSpace")
}

allow if {
	input.operation.nsid == "town.muni.arbiter.setArbiterConfig"
	requester_rank >= access_rank("Owner")
}

allow if {
	input.operation.nsid == "town.muni.arbiter.setSpaceMemberAccess"
	requester_rank >= access_rank("AddMembers")
	access_rank(input.operation.params.access.level) <= requester_rank
	not target_exists_in_raw
}

allow if {
	input.operation.nsid == "town.muni.arbiter.setSpaceMemberAccess"
	requester_rank >= access_rank("AddMembers")
	access_rank(input.operation.params.access.level) <= requester_rank
	target_exists_in_raw
	requester_rank >= access_rank("RemoveMembers")
	raw_target_rank <= requester_rank
}

allow if {
	input.operation.nsid == "town.muni.arbiter.removeSpaceMember"
	requester_rank >= access_rank("RemoveMembers")
	target_exists_in_raw
	raw_target_rank <= requester_rank
}

allow if {
	input.operation.nsid == "town.muni.arbiter.deleteArbiter"
	requester_rank >= access_rank("Owner")
	count(space_members("$admin")) == 1
}
