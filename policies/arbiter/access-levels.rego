# Default access-level authorization policy for the Muni Town Arbiter.
#
# This policy evaluates whether a given XRPC operation is allowed. It
# receives:
#
#   data.arbiter.config       — the arbiter's configuration object
#   input.caller.did         — the requester's DID
#   input.operation.nsid     — the XRPC method NSID
#   input.operation.params   — the method parameters
#
# The policy queries space membership data on-demand via:
#   xrpc_local(path, params)   — query the local arbiter (native NSIDs resolved internally)
#   xrpc_remote(did, path, params) — query a remote arbiter
#
# XRPC queries from the policy are ALWAYS read-only (never procedures).
# Space membership data is NOT passed upfront — it's fetched via xrpc_local
# as needed, avoiding the overhead of passing thousands of members to every
# policy evaluation.

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
# Space data access via xrpc_local (on-demand queries)
# ---------------------------------------------------------------------------

# Fetch space members via local XRPC — this suspends the policy VM and the
# IO layer resolves it by querying the arbiter core directly (bypassing policy).
# Fetch space members via local XRPC — returns the full response object.
# The policy extracts the `.members` field.
space_members(space_key) := members if {
	resp := xrpc_local("town.muni.arbiter.getSpaceMembers", {"spaceKey": space_key})
	members := resp.members
}

# Fetch space config via local XRPC — extracts the `.config` field.
space_config(space_key) := config if {
	resp := xrpc_local("town.muni.arbiter.getSpaceConfig", {"spaceKey": space_key})
	config := resp.config
}

# ---------------------------------------------------------------------------
# Resolved members via local/remote delegation
# ---------------------------------------------------------------------------

# Direct member in the target space
direct_members contains member if {
	some entry in space_members(input.operation.params.spaceKey)
	member := {"did": entry.did, "access": entry.access, "via": input.operation.params.spaceKey}
}

# Direct member inherited from $admin space
direct_members contains member if {
	input.operation.params.spaceKey != "$admin"
	some entry in space_members("$admin")
	member := {"did": entry.did, "access": entry.access, "via": "$admin"}
}

# Delegated from a remote space: the member DID is "<arbiterDid>|<spaceKey>"
remote_delegation contains member if {
	some entry in space_members(input.operation.params.spaceKey)
	contains(entry.did, "|")
	parts := split(entry.did, "|")
	arbiter_did := parts[0]
	space_key := parts[1]

	# This resolves asynchronously via xrpc_remote -> __builtin_host_await
	resp := xrpc_remote(arbiter_did, "town.muni.arbiter.resolveSpaceMembers", {"spaceKey": space_key})
	some remote_entry in resp.members
	member := {
		"did": remote_entry.did,
		"access": min_access(remote_entry.access, entry.access),
		"via": concat("|", [arbiter_did, space_key]),
	}
}

# Delegated from a remote space in the admin space
remote_delegation contains member if {
	input.operation.params.spaceKey != "$admin"
	some entry in space_members("$admin")
	contains(entry.did, "|")
	parts := split(entry.did, "|")
	arbiter_did := parts[0]
	space_key := parts[1]
	resp := xrpc_remote(arbiter_did, "town.muni.arbiter.resolveSpaceMembers", {"spaceKey": space_key})
	some remote_entry in resp.members
	member := {
		"did": remote_entry.did,
		"access": min_access(remote_entry.access, entry.access),
		"via": concat("|", [arbiter_did, space_key]),
	}
}

# Recursive delegation expansion using depth-limited function.
# expand_delegation(space_key, parent_access, depth) returns an array of all
# resolved members for a given space, capped at [depth] levels deep.
# Terminal entries (real DIDs) are emitted directly; space:<key> entries are
# expanded recursively with depth-1.

# -- expand_delegation helpers -------------------------------------------------

# Terminal entries: real DIDs (not space: and not remote |)
_expand_terminal(child_key, parent_access) := [member |
	some entry in space_members(child_key)
	not startswith(entry.did, "space:")
	not contains(entry.did, "|")
	member := {
		"did": entry.did,
		"access": min_access(entry.access, parent_access),
		"via": child_key,
	}
]

# Remote delegation entries: arbiter|space references resolved via xrpc_remote
_expand_remote(child_key, parent_access) := [member |
	some entry in space_members(child_key)
	contains(entry.did, "|")
	parts := split(entry.did, "|")
	arbiter_did := parts[0]
	space_key := parts[1]
	resp := xrpc_remote(arbiter_did, "town.muni.arbiter.resolveSpaceMembers", {"spaceKey": space_key})
	some remote_entry in resp.members
	member := {
		"did": remote_entry.did,
		"access": min_access(remote_entry.access, min_access(entry.access, parent_access)),
		"via": concat("|", [arbiter_did, space_key]),
	}
]

# Recursive entries: space:<key> references expanded with depth-1
_expand_recursive(child_key, parent_access, depth) := [grandchild |
	some entry in space_members(child_key)
	startswith(entry.did, "space:")
	child := trim_prefix(entry.did, "space:")
	some grandchild in expand_delegation(child, min_access(entry.access, parent_access), depth - 1)
]

expand_delegation(child_key, parent_access, depth) := result if {
	depth > 0
	terminal := _expand_terminal(child_key, parent_access)
	remote := _expand_remote(child_key, parent_access)
	recursive := _expand_recursive(child_key, parent_access, depth)
	combined := array.concat(terminal, remote)
	result := array.concat(combined, recursive)
}

# Combine everything into resolved_members_raw
resolved_members_raw contains member if {
	some member in direct_members
}

resolved_members_raw contains member if {
	some member in remote_delegation
}

resolved_members_raw contains member if {
	some entry in direct_members
	startswith(entry.did, "space:")
	some member in expand_delegation(trim_prefix(entry.did, "space:"), entry.access, 9)
}

resolved_members_raw contains member if {
	some entry in remote_delegation
	startswith(entry.did, "space:")
	some member in expand_delegation(trim_prefix(entry.did, "space:"), entry.access, 9)
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
	not startswith(member.did, "space:")
	not contains(member.did, "|")
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
	resp := xrpc_remote(arbiter_did, "town.muni.arbiter.resolveSpaceMembers", {"spaceKey": space_key})
	count(resp.members) == 0
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
	input.operation.params.spaceKey != "$admin"
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

allow if {
	input.operation.nsid == "town.muni.arbiter.updateDidDoc"
	requester_rank >= access_rank("Owner")
}
