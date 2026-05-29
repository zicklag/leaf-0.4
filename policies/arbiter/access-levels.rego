# Default access-level authorization policy for the Muni Town Arbiter.
#
# This policy evaluates whether a given XRPC operation is allowed and
# returns the full XRPC response.  It receives:
#
#   data.arbiter.config            — the arbiter's configuration object
#   input.caller.did              — the requester's DID
#   input.operation.nsid          — the XRPC method NSID
#   input.operation.method        — "query" or "procedure"
#   input.operation.params        — the method parameters
#
# The policy queries space membership data on-demand via:
#   xrpc_local(method, path, params)  — query the local arbiter
#   xrpc_remote(did, method, path, params) — query a remote arbiter
#
# The single entry point is `data.arbiter.response`, which MUST return
# an object with `body` and `status` fields.

package arbiter

import rego.v1

# This list of XRPC endpoints that are processed locally by the arbiter
arbiter_xrpc_nsids := {
	"town.muni.arbiter.getArbiterConfig",
	"town.muni.arbiter.setArbiterConfig",
	"town.muni.arbiter.deleteArbiter",
	"town.muni.arbiter.createSpace",
	"town.muni.arbiter.getSpaceConfig",
	"town.muni.arbiter.setSpaceConfig",
	"town.muni.arbiter.deleteSpace",
	"town.muni.arbiter.listSpaces",
	"town.muni.arbiter.getSpaceMembers",
	"town.muni.arbiter.setSpaceMemberAccess",
	"town.muni.arbiter.removeSpaceMember",
}

# The backend service is where we will proxy XRPC requests to if they are not handled
# by the arbiter or the policy itself. We use the configured backend service if there is one.
backend_service := data.arbiter.config.backend_service

# We default to the ATProto PDS associated to the arbiter's DID if there is no configured backend service.
backend_service := concat("#", [data.arbiter.did, "atproto_pds"]) if not data.arbiter.config.backend_service

# When a request is not allowed, the response is a permission denied error
response := {"status": 403, "body": {"error": "ErrPermissionDenied"}} if not allow

# Response for XRPCs handled natively by the arbiter
response := xrpc_local(input.operation.method, input.operation.nsid, input.operation.params) if {
	allow
	input.operation.nsid in arbiter_xrpc_nsids
}

# Response for space member resolution, which the policy processes itself
else := {
	"status": 200,
	"body": {"members": resolved_members, "missingSpaces": missing_spaces},
} if {
	allow
	input.operation.nsid == "town.muni.arbiter.resolveSpaceMembers"
}

# Proxy XRPC requests to the backend service if no other rule matches
else := xrpc_remote(
	backend_service,
	input.operation.method,
	input.operation.nsid,
	input.operation.params,
) if allow

# ---------------------------------------------------------------------------
# Authorization (internal — still uses `allow` as a helper)
# ---------------------------------------------------------------------------

# --- Reads ---
#
# By default we do not allow a request
default allow := false

# Public member list: anyone can read
allow if {
	input.operation.nsid in {"town.muni.arbiter.resolveSpaceMembers", "town.muni.arbiter.getSpaceMembers"}
	space_config(input.operation.params.spaceType, input.operation.params.spaceKey).publicMembers == true
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
	space_config(input.operation.params.spaceType, input.operation.params.spaceKey).publicRecords == true
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
	count(space_members("town.muni.arbiter.config.adminSpace", "$admin")) == 1
}

allow if {
	input.operation.nsid == "town.muni.arbiter.updateDidDoc"
	requester_rank >= access_rank("Owner")
}

# Allow ATProto record reads unconditionally. They will get forwarded to the backend DID if any.
allow if {
	input.operation.nsid == "com.atproto.repo.getRecord"
}

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

# Fetch space members via local XRPC — returns the full response object.
# The policy extracts the `.members` field.
space_members(space_type, space_key) := members if {
	resp := xrpc_local("query", "town.muni.arbiter.getSpaceMembers", {"spaceKey": space_key, "spaceType": space_type}).body
	members := resp.members
}

# Fetch space config via local XRPC — extracts the `.config` field.
space_config(space_type, space_key) := config if {
	resp := xrpc_local("query", "town.muni.arbiter.getSpaceConfig", {"spaceKey": space_key, "spaceType": space_type}).body
	config := resp.config
}

# ---------------------------------------------------------------------------
# Resolved members via local/remote delegation
# ---------------------------------------------------------------------------

# Direct member in the target space
direct_members contains member if {
	some entry in space_members(input.operation.params.spaceType, input.operation.params.spaceKey)
	member := {
		"did": entry.did,
		"access": entry.access,
		"via": concat("/", [input.operation.params.spaceType, input.operation.params.spaceKey]),
	}
}

# Direct member inherited from $admin space
direct_members contains member if {
	input.operation.params.spaceKey != "$admin"
	some entry in space_members("town.muni.arbiter.config.adminSpace", "$admin")
	member := {
		"did": entry.did,
		"access": entry.access,
		"via": concat("/", ["town.muni.arbiter.config.adminSpace", "$admin"]),
	}
}

# Delegated from a remote space: the member DID is "<arbiterDid>|<spaceType>|<spaceKey>"
remote_delegation contains member if {
	some entry in space_members(input.operation.params.spaceType, input.operation.params.spaceKey)
	contains(entry.did, "|")
	parts := split(entry.did, "|")
	arbiter_did := parts[0]
	remote_space_type := parts[1]
	remote_space_key := parts[2]

	# This resolves asynchronously via xrpc_remote -> __builtin_host_await
	resp := xrpc_remote(
		arbiter_did,
		"query",
		"town.muni.arbiter.resolveSpaceMembers",
		{"spaceKey": remote_space_key, "spaceType": remote_space_type},
	).body

	some remote_entry in resp.members
	member := {
		"did": remote_entry.did,
		"access": min_access(remote_entry.access, entry.access),
		"via": concat("|", [arbiter_did, remote_space_type, remote_space_key]),
	}
}

# Delegated from a remote space in the admin space
remote_delegation contains member if {
	input.operation.params.spaceKey != "$admin"
	some entry in space_members("town.muni.arbiter.config.adminSpace", "$admin")
	contains(entry.did, "|")
	parts := split(entry.did, "|")
	arbiter_did := parts[0]
	remote_space_type := parts[1]
	remote_space_key := parts[2]
	resp := xrpc_remote(
		arbiter_did,
		"query",
		"town.muni.arbiter.resolveSpaceMembers",
		{"spaceKey": remote_space_key, "spaceType": remote_space_type},
	).body

	some remote_entry in resp.members
	member := {
		"did": remote_entry.did,
		"access": min_access(remote_entry.access, entry.access),
		"via": concat("|", [arbiter_did, remote_space_type, remote_space_key]),
	}
}

# Recursive delegation expansion using depth-limited function.
# expand_delegation(space_key, parent_access, depth) returns an array of all
# resolved members for a given space, capped at [depth] levels deep.
# Terminal entries (real DIDs) are emitted directly; space:<key> entries are
# expanded recursively with depth-1.

# -- expand_delegation helpers -------------------------------------------------

# Terminal entries: real DIDs (not space: and not remote |)
_expand_terminal(child_type, child_key, parent_access) := [member |
	some entry in space_members(child_type, child_key)
	not startswith(entry.did, "space:")
	not contains(entry.did, "|")
	member := {
		"did": entry.did,
		"access": min_access(entry.access, parent_access),
		"via": concat("/", [child_type, child_key]),
	}
]

# Remote delegation entries: arbiter|spaceType|spaceKey references resolved via xrpc_remote
_expand_remote(child_type, child_key, parent_access) := [member |
	some entry in space_members(child_type, child_key)
	contains(entry.did, "|")
	parts := split(entry.did, "|")
	arbiter_did := parts[0]
	remote_type := parts[1]
	remote_key := parts[2]
	resp := xrpc_remote(
		arbiter_did,
		"query",
		"town.muni.arbiter.resolveSpaceMembers",
		{"spaceKey": remote_key, "spaceType": remote_type},
	).body
	some remote_entry in resp.members
	member := {
		"did": remote_entry.did,
		"access": min_access(remote_entry.access, min_access(entry.access, parent_access)),
		"via": concat("|", [arbiter_did, remote_type, remote_key]),
	}
]

# Recursive entries: space:<spaceType>/<key> references expanded with depth-1
_expand_recursive(child_type, child_key, parent_access, depth) := [grandchild |
	some entry in space_members(child_type, child_key)
	startswith(entry.did, "space:")
	child_full := trim_prefix(entry.did, "space:")
	child_parts := split(child_full, "/")
	grandchild_type := child_parts[0]
	grandchild_key := child_parts[1]
	some grandchild in expand_delegation(
		grandchild_type,
		grandchild_key,
		min_access(entry.access, parent_access),
		depth - 1,
	)
]

expand_delegation(child_type, child_key, parent_access, depth) := result if {
	depth > 0
	terminal := _expand_terminal(child_type, child_key, parent_access)
	remote := _expand_remote(child_type, child_key, parent_access)
	recursive := _expand_recursive(child_type, child_key, parent_access, depth)
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
	child_full := trim_prefix(entry.did, "space:")
	child_parts := split(child_full, "/")
	some member in expand_delegation(child_parts[0], child_parts[1], entry.access, 9)
}

resolved_members_raw contains member if {
	some entry in remote_delegation
	startswith(entry.did, "space:")
	child_full := trim_prefix(entry.did, "space:")
	child_parts := split(child_full, "/")
	some member in expand_delegation(child_parts[0], child_parts[1], entry.access, 9)
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

missing_spaces := []

# ---------------------------------------------------------------------------
# Target member helpers (for set/remove)
# ---------------------------------------------------------------------------

target_exists_in_raw if {
	some entry in space_members(input.operation.params.spaceType, input.operation.params.spaceKey)
	entry.did == input.operation.params.memberDid
}

raw_target_rank := rank if {
	some entry in space_members(input.operation.params.spaceType, input.operation.params.spaceKey)
	entry.did == input.operation.params.memberDid
	rank := member_rank(entry)
}
