# New Arbiter Plan

The new plan for the arbiter is simpler than before, and yet combines more fucntionality.

The idea is that we hav a policy-core that allows us to execute a pluggable auth policy that can be used to filter XRPC calls based on almost any criteria, even depending on making other XRPC calls to local or remote services.

Our next step is to update the simulator to reflect this.

We have in ./lexicons the definition for the new arbiter lexicons. While previous designs of the arbiter had authentication mechanisms built specifically around those needs reflected in the lexicon, we realized that even authenticating calls like the default ATProto getRecord and putRecord requests can be fitlered on using the same policy.

It's just a matter of applying the policy as a protection over the XRPC methods.

So we want to update our arbiter-simultor to use the new policy-core-wasm crate, and to simulate the arbiters, very similar to how it is done now, but using that new perspective. Do you have any questions?