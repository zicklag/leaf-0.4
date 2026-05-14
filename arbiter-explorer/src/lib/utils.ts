import type {
  Access,
  Member,
  Message,
  MessageKind,
  SpaceId,
} from "arbiter-wasm";
import { ALL_ACCESSES } from "./types";

// ---------------------------------------------------------------------------
// Message builders
// ---------------------------------------------------------------------------

export function buildMessage(
  userDid: string,
  arbiterDid: string,
  spaceKey: string,
  kind: MessageKind,
  resolverDepth = 5,
): Message {
  return {
    userDid,
    arbiterDid,
    spaceKey,
    srcJobId: 0,
    resolverDepth,
    kind,
  };
}

// ---------------------------------------------------------------------------
// Member parsing
// ---------------------------------------------------------------------------

export function parseMember(raw: string): Member | null {
  // Try User format: just a DID string
  if (raw.includes("did:")) {
    return { tag: "MemberUser", value: raw };
  }
  // Try RemoteSpace format: arbiterDid/spaceKey
  if (raw.includes("/")) {
    const slash = raw.indexOf("/");
    return {
      tag: "MemberRemoteSpace",
      value: { arbiterDid: raw.substring(0, slash), spaceKey: raw.substring(slash + 1) },
    };
  }
  // Otherwise treat as local space key
  return { tag: "MemberLocalSpace", value: raw };
}

/** Build a Member from a member entry (as stored in MemberEntryView). */
export function buildMemberFromEntry(entry: {
  memberType: string;
  value: string;
}): Member | null {
  switch (entry.memberType) {
    case "User":
      return { tag: "MemberUser", value: entry.value };
    case "LocalSpace":
      return { tag: "MemberLocalSpace", value: entry.value };
    case "RemoteSpace": {
      const sid = parseSpaceId(entry.value);
      if (!sid) return null;
      return { tag: "MemberRemoteSpace", value: sid };
    }
    default:
      return null;
  }
}

export function parseSpaceId(raw: string): SpaceId | null {
  // SpaceId for remote spaces: arbiterDid/spaceKey
  const slash = raw.lastIndexOf("/");
  if (slash < 0) return null;
  return {
    arbiterDid: raw.substring(0, slash),
    spaceKey: raw.substring(slash + 1),
  };
}

export function memberDisplay(member: Member): string {
  switch (member.tag) {
    case "MemberUser":
      return `👤 ${shortDid(member.value)}`;
    case "MemberLocalSpace":
      return `📁 ${member.value}`;
    case "MemberRemoteSpace":
      return `🌐 ${member.value}`;
  }
}

export function memberTypeLabel(type: string): string {
  switch (type) {
    case "User":
      return "User";
    case "LocalSpace":
      return "Local Space";
    case "RemoteSpace":
      return "Remote Space";
    default:
      return type;
  }
}

// ---------------------------------------------------------------------------
// Display helpers
// ---------------------------------------------------------------------------

export function shortDid(did: string, maxLen = 24): string {
  if (did.length <= maxLen) return did;
  return did.slice(0, maxLen - 3) + "…";
}

export function accessLabel(access: Access): string {
  return access;
}

export function accessLevel(access: Access): number {
  return ALL_ACCESSES.lastIndexOf(access)!;
}

/** Convert an Access string to Rust's adjacently tagged JSON format */
export function accessTag(access: Access): { tag: string; value: number } {
  return { tag: access, value: accessLevel(access) };
}

export function accessColor(access: Access): string {
  const level = accessLevel(access);
  const total = ALL_ACCESSES.length - 1;
  const ratio = total > 0 ? level / total : 0;
  // Perceptually uniform OKLCH: cool teal (low access) → warm amber (high access).
  // Chroma and lightness stay in a range that reads clearly on light backgrounds.
  const h = 200 - ratio * 160; // 200° (teal) → 40° (amber)
  const c = 0.10 + ratio * 0.12; // chroma rises with level
  const l = 0.70 - ratio * 0.15; // lightness drops slightly
  return `oklch(${l.toFixed(2)} ${c.toFixed(2)} ${h.toFixed(0)})`;
}

// ---------------------------------------------------------------------------
// ID generation
// ---------------------------------------------------------------------------

let userIdCounter = 0;
export function generateUserId(label: string): string {
  return label.toLowerCase().replace(/\s+/g, '-');
}

let arbiterIdCounter = 0;
export function generateArbiterDid(): string {
  arbiterIdCounter++;
  return `arbiter${arbiterIdCounter}`;
}

// ---------------------------------------------------------------------------
// Initials for avatars
// ---------------------------------------------------------------------------

export function userInitial(label: string): string {
  const parts = label.split(/\s+/);
  if (parts.length >= 2) {
    return (parts[0][0] + parts[1][0]).toUpperCase();
  }
  return label.slice(0, 2).toUpperCase();
}

export function userColor(label: string): string {
  // Deterministic color based on the name
  let hash = 0;
  for (let i = 0; i < label.length; i++) {
    hash = label.charCodeAt(i) + ((hash << 5) - hash);
  }
  const h = Math.abs(hash % 360);
  return `hsl(${h}, 45%, 60%)`;
}
