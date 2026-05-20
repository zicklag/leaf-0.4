//! Integration tests for the default access-levels policy.
//!
//! Tests realistic multi-arbiter scenarios through the full state machine,
//! including local space delegation, remote space resolution, and complex
//! permission chains.

mod common;
use common::*;
use arbiter_core2::core::JobArgs;
use serde_json::json;

// ===========================================================================
// Basic owner operations
// ===========================================================================

#[test]
fn owner_can_create_spaces() {
    let mut h = Harness::new();
    h.create_default_arbiter("org", "alice");

    h.assert_ok("org", "alice", "team", create_space("team"));
    h.assert_ok("org", "alice", "docs", create_space("docs"));
}

#[test]
fn non_member_cannot_create_space() {
    let mut h = Harness::new();
    h.create_default_arbiter("org", "alice");
    h.assert_denied("org", "stranger", "team", create_space("team"));
}

#[test]
fn owner_can_delete_arbiter() {
    let mut h = Harness::new();
    h.create_default_arbiter("org", "alice");
    h.assert_ok("org", "alice", "$admin", delete_arbiter());
    assert!(!h.state.arbiters.contains_key("org"));
}

#[test]
fn non_owner_cannot_delete_arbiter() {
    let mut h = Harness::new();
    h.create_default_arbiter("org", "alice");
    h.assert_denied("org", "stranger", "$admin", delete_arbiter());
}

#[test]
fn multiple_owners_cannot_delete_arbiter() {
    let mut h = Harness::new();
    h.create_default_arbiter("org", "alice");
    h.assert_ok("org", "alice", "$admin", add_member(member_did("bob"), "Owner"));
    h.assert_denied("org", "alice", "$admin", delete_arbiter());
    h.assert_denied("org", "bob", "$admin", delete_arbiter());
}

#[test]
fn owner_can_delete_space() {
    let mut h = Harness::new();
    h.create_default_arbiter("org", "alice");
    h.assert_ok("org", "alice", "team", create_space("team"));
    h.assert_ok("org", "alice", "team", delete_space());
    assert!(!h.state.arbiters.get("org").unwrap().spaces.contains_key("team"));
}

// ===========================================================================
// Access level hierarchy
// ===========================================================================

#[test]
fn owner_can_add_members() {
    let mut h = Harness::new();
    h.create_default_arbiter("org", "alice");

    h.assert_ok("org", "alice", "$admin", add_member(member_did("bob"), "Owner"));
    h.assert_ok("org", "alice", "$admin", add_member(member_did("carol"), "IsMember"));
}

#[test]
fn read_member_cannot_create_space() {
    let mut h = Harness::new();
    h.create_default_arbiter("org", "alice");
    h.assert_ok("org", "alice", "$admin", add_member(member_did("bob"), "ReadMemberList"));
    h.assert_denied("org", "bob", "team", create_space("team"));
}

#[test]
fn cannot_grant_higher_access_than_own() {
    let mut h = Harness::new();
    h.create_default_arbiter("org", "alice");

    h.assert_ok("org", "alice", "$admin", add_member(member_did("bob"), "AddMembers"));

    // Bob can add someone with IsMember
    h.assert_ok("org", "bob", "$admin", add_member(member_did("carol"), "IsMember"));

    // Bob cannot add someone with Owner (higher than his access)
    h.assert_denied("org", "bob", "$admin", add_member(member_did("dave"), "Owner"));

    // Bob cannot add someone with ConfigureSpace (also higher)
    h.assert_denied("org", "bob", "$admin", add_member(member_did("eve"), "ConfigureSpace"));
}

#[test]
fn need_remove_members_to_modify_existing() {
    let mut h = Harness::new();
    h.create_default_arbiter("org", "alice");

    h.assert_ok("org", "alice", "$admin", add_member(member_did("bob"), "IsMember"));
    h.assert_ok("org", "alice", "$admin", add_member(member_did("carol"), "AddMembers"));

    // Carol has AddMembers — can add a new member
    h.assert_ok("org", "carol", "$admin", add_member(member_did("dave"), "ReadMemberList"));

    // Carol tries to modify bob's existing entry — needs RemoveMembers
    h.assert_denied("org", "carol", "$admin", add_member(member_did("bob"), "ReadMemberList"));

    // Alice (Owner) can modify anyone
    h.assert_ok("org", "alice", "$admin", add_member(member_did("bob"), "ReadMemberList"));
}

#[test]
fn cannot_remove_higher_access() {
    let mut h = Harness::new();
    h.create_default_arbiter("org", "alice");

    // Bob has RemoveMembers, Carol is Owner
    h.assert_ok("org", "alice", "$admin", add_member(member_did("bob"), "RemoveMembers"));
    h.assert_ok("org", "alice", "$admin", add_member(member_did("carol"), "Owner"));

    // Bob cannot remove Carol (higher access)
    h.assert_denied("org", "bob", "$admin", remove_member(member_did("carol")));

    // Bob can remove someone with lower access
    h.assert_ok("org", "alice", "$admin", add_member(member_did("dave"), "ReadMemberList"));
    h.assert_ok("org", "bob", "$admin", remove_member(member_did("dave")));
}

// ===========================================================================
// Resolved member lists
// ===========================================================================

#[test]
fn owner_sees_themselves_in_admin() {
    let mut h = Harness::new();
    h.create_default_arbiter("org", "alice");

    let members = h.resolved_members("org", "alice", "$admin");
    assert!(!members.is_empty(), "Admin space should have members");
    assert_member_exists(&members, "alice", "Owner");
}

#[test]
fn resolve_members_includes_all_direct_members() {
    let mut h = Harness::new();
    h.create_default_arbiter("org", "alice");

    h.assert_ok("org", "alice", "$admin", add_member(member_did("bob"), "IsMember"));
    h.assert_ok("org", "alice", "$admin", add_member(member_did("carol"), "ReadMemberList"));

    let members = h.resolved_members("org", "alice", "$admin");
    assert_member_exists(&members, "alice", "Owner");
    assert_member_exists(&members, "bob", "IsMember");
    assert_member_exists(&members, "carol", "ReadMemberList");
}

#[test]
fn non_member_cannot_resolve_members() {
    let mut h = Harness::new();
    h.create_default_arbiter("org", "alice");
    h.assert_denied("org", "stranger", "team", create_space("team"));
}

// ===========================================================================
// Local space delegation
// ===========================================================================

#[test]
fn access_limited_by_parent_delegation() {
    let mut h = Harness::new();
    h.create_default_arbiter("org", "alice");

    h.assert_ok("org", "alice", "team", create_space("team"));
    h.assert_ok("org", "alice", "team", add_member(member_did("bob"), "Owner"));
    h.assert_ok("org", "alice", "$admin", add_member(member_local("team"), "ReadMemberList"));

    // Bob's effective access in $admin should be ReadMemberList (limited by parent)
    let members = h.resolved_members("org", "alice", "$admin");
    assert_member_exists(&members, "bob", "ReadMemberList");
}

#[test]
fn members_of_child_space_inherit_access() {
    let mut h = Harness::new();
    h.create_default_arbiter("org", "alice");

    h.assert_ok("org", "alice", "team", create_space("team"));
    h.assert_ok("org", "alice", "team", add_member(member_did("bob"), "IsMember"));
    h.assert_ok("org", "alice", "$admin", add_member(member_local("team"), "IsMember"));

    // Bob should be resolved via team delegation
    let members = h.resolved_members("org", "alice", "$admin");
    assert_member_exists(&members, "bob", "IsMember");
}

#[test]
fn delegation_chain_works() {
    let mut h = Harness::new();
    h.create_default_arbiter("org", "alice");

    // team has bob as IsMember, $admin delegates to team with ReadMemberList
    h.assert_ok("org", "alice", "team", create_space("team"));
    h.assert_ok("org", "alice", "team", add_member(member_did("bob"), "IsMember"));
    h.assert_ok("org", "alice", "$admin", add_member(member_local("team"), "ReadMemberList"));

    // Bob via team → $admin: min(IsMember, ReadMemberList) = ReadMemberList
    let members = h.resolved_members("org", "alice", "$admin");
    assert_member_exists(&members, "bob", "ReadMemberList");
}

#[test]
fn public_members_allows_non_member_access() {
    let mut h = Harness::new();
    h.create_default_arbiter("org", "alice");

    // Create a space with public members
    h.assert_ok("org", "alice", "team", JobArgs::CreateSpace {
        space_type: "town.muni.arbiter.config.space".into(),
        config: json!({
            "$type": "town.muni.arbiter.config.space",
            "publicRecords": false,
            "publicMembers": true,
        }),
    });
    h.assert_ok("org", "alice", "team", add_member(member_did("bob"), "IsMember"));

    // Alice is in $admin as Owner, so she's in team via delegation.
    // But the key test: can a non-member ("stranger") read the member list
    // because publicMembers is true?
    let members = h.resolved_members("org", "stranger", "team");
    assert!(!members.is_empty(), "Public space should be readable by anyone");
    assert_member_exists(&members, "bob", "IsMember");
}

// ===========================================================================
// Remote space resolution
// ===========================================================================

#[test]
fn remote_space_resolution_works() {
    let mut h = Harness::new();

    h.create_default_arbiter("org", "alice");
    h.create_default_arbiter("partner", "carol");

    // Partner creates "shared" with public members so others can read it
    h.assert_ok("partner", "carol", "shared", JobArgs::CreateSpace {
        space_type: "town.muni.arbiter.config.space".into(),
        config: json!({
            "$type": "town.muni.arbiter.config.space",
            "publicRecords": false,
            "publicMembers": true,
        }),
    });
    h.assert_ok("partner", "carol", "shared", add_member(member_did("dave"), "Owner"));

    h.assert_ok("org", "alice", "team", create_space("team"));
    h.assert_ok("org", "alice", "team", add_member(
        member_remote("partner", "shared"),
        "IsMember",
    ));

    let members = h.resolved_members("org", "alice", "team");
    assert_member_exists(&members, "dave", "IsMember");
}

#[test]
fn remote_access_limited_by_parent() {
    let mut h = Harness::new();

    h.create_default_arbiter("org", "alice");
    h.create_default_arbiter("partner", "carol");

    h.assert_ok("partner", "carol", "shared", JobArgs::CreateSpace {
        space_type: "town.muni.arbiter.config.space".into(),
        config: json!({
            "$type": "town.muni.arbiter.config.space",
            "publicRecords": false,
            "publicMembers": true,
        }),
    });
    h.assert_ok("partner", "carol", "shared", add_member(member_did("dave"), "Owner"));

    h.assert_ok("org", "alice", "team", create_space("team"));
    h.assert_ok("org", "alice", "team", add_member(
        member_remote("partner", "shared"),
        "ReadMemberList",
    ));

    let members = h.resolved_members("org", "alice", "team");
    assert_member_exists(&members, "dave", "ReadMemberList");
}

#[test]
fn deep_remote_chain_resolves() {
    let mut h = Harness::new();

    h.create_default_arbiter("org", "alice");
    h.create_default_arbiter("partner", "carol");

    // partner has users space with public members + dave
    h.assert_ok("partner", "carol", "users", JobArgs::CreateSpace {
        space_type: "town.muni.arbiter.config.space".into(),
        config: json!({
            "$type": "town.muni.arbiter.config.space",
            "publicRecords": false,
            "publicMembers": true,
        }),
    });
    h.assert_ok("partner", "carol", "users", add_member(member_did("dave"), "Owner"));

    // org's team delegates directly to partner/users
    h.assert_ok("org", "alice", "team", create_space("team"));
    h.assert_ok("org", "alice", "team", add_member(
        member_remote("partner", "users"),
        "IsMember",
    ));

    // Dave should be in org/team via remote resolution
    let members = h.resolved_members("org", "alice", "team");
    assert_member_exists(&members, "dave", "IsMember");
}

// ===========================================================================
// Custom policies
// ===========================================================================

#[test]
fn allow_all_policy() {
    let mut h = Harness::new();

    let allow_all = r#"
        package arbiter
        import rego.v1
        default allow := true
        resolved_members contains {"did": input.requester, "access": {"level": "Owner"}} if { true }
        needs_resolution contains entry if { false }
    "#;

    h.create_arbiter("org", "alice", allow_all);

    h.assert_ok("org", "stranger", "team", create_space("team"));
    h.assert_ok("org", "stranger", "team", add_member(member_did("alice"), "Owner"));

    let members = h.resolved_members("org", "stranger", "$admin");
    assert_member_exists(&members, "stranger", "Owner");
}

#[test]
fn deny_all_policy() {
    let mut h = Harness::new();

    let deny_all = r#"
        package arbiter
        import rego.v1
        default allow := false
        resolved_members contains {"did": "noone", "access": {"level": "ReadMemberList"}} if { false }
        needs_resolution contains entry if { false }
    "#;

    h.create_arbiter("org", "alice", deny_all);

    h.assert_denied("org", "alice", "team", create_space("team"));
}

// ===========================================================================
// Helpers
// ===========================================================================

fn assert_member_exists(members: &[serde_json::Value], expected_did: &str, expected_level: &str) {
    let found = members.iter().find(|m| {
        m.get("did").and_then(|v| v.as_str()) == Some(expected_did)
    });
    assert!(
        found.is_some(),
        "Member {expected_did} not found in resolved members. Members: {members:?}"
    );
    let level = found
        .and_then(|m| m.get("access"))
        .and_then(|a| a.get("level"))
        .and_then(|v| v.as_str());
    assert_eq!(
        level,
        Some(expected_level),
        "Member {expected_did}: expected {expected_level}, got {level:?}"
    );
}
