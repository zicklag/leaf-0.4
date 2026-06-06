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

# When a request is not allowed, the response is a permission denied error
response := {"status": 403, "body": {"error": "ErrPermissionDenied"}} if not allow

# When a local request is allowed, we have the arbiter itself handle it using xrpc_local
response := xrpc_local(input.operation.method, input.operation.nsid, input.operation.params) if {
	allow
	input.operation.nsid in arbiter_xrpc_nsids
}

# By default we do not allow a request
default allow := false

# The arbiter account itself is always allowed
allow if input.caller.did == data.arbiter.did

# The owner is always allowed
allow if input.caller.did == "${owner}"
