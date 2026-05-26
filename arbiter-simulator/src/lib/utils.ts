import type {
  Access,
  ServerSnapshot,
  ArbiterSnapshot,
  MemberEntry,
} from "./types";
import { ALL_ACCESSES } from "./types";

// ---------------------------------------------------------------------------
// Access level helpers
// ---------------------------------------------------------------------------

/** Extract access level string from an access config object. */
export function accessLevelStr(access: Record<string, unknown>): string {
  if (typeof access.level === 'string') return access.level;
  if (typeof access === 'string') return access;
  return 'ReadMemberList';
}

/** Get numeric access level index. */
export function accessLevelNum(access: Record<string, unknown>): number {
  const str = accessLevelStr(access);
  const idx = ALL_ACCESSES.indexOf(str as Access);
  return idx >= 0 ? idx : 0;
}

/** Get a human-readable label for an access config. */
export function accessLabel(access: Record<string, unknown>): string {
  const str = accessLevelStr(access);
  return str;
}

/** Get a color for an access level. */
export function accessColor(access: Record<string, unknown>): string {
  const level = accessLevelNum(access);
  const total = ALL_ACCESSES.length - 1;
  const ratio = total > 0 ? level / total : 0;
  const h = 200 - ratio * 160;
  const c = 0.10 + ratio * 0.12;
  const l = 0.70 - ratio * 0.15;
  return `oklch(${l.toFixed(2)} ${c.toFixed(2)} ${h.toFixed(0)})`;
}

// ---------------------------------------------------------------------------
// Display helpers
// ---------------------------------------------------------------------------

export function shortDid(did: string, maxLen = 24): string {
  if (did.length <= maxLen) return did;
  return did.slice(0, maxLen - 3) + "…";
}

/** Get a human-readable label for a member entry. */
export function memberDisplay(member: MemberEntry): string {
  if (member.did.startsWith('space:')) {
    return `📁 ${member.did.slice(6)}`;
  }
  if (member.did.includes('|')) {
    const [arb, key] = member.did.split('|', 2);
    return `🌐 ${arb}/${key}`;
  }
  return `👤 ${shortDid(member.did)}`;
}

/** Get spaces from the server snapshot for a given arbiter. */
export function getArbiterSpaces(
  snapshot: ServerSnapshot,
  arbiterDid: string,
) {
  return snapshot.arbiters.find((a) => a.did === arbiterDid)?.spaces ?? [];
}

/** Build a member DID from type + value for setSpaceMemberAccess. */
export function buildMemberDid(
  memberType: 'user' | 'localspace' | 'remotespace',
  value: string,
  remoteArbiterDid?: string,
): string {
  switch (memberType) {
    case 'user':
      return value;
    case 'localspace':
      return `space:${value}`;
    case 'remotespace':
      return `${remoteArbiterDid}|${value}`;
  }
}

/** Parse a member DID back into parts for display. */
export function parseMemberDid(
  did: string,
): { kind: 'user' | 'localspace' | 'remotespace'; display: string } {
  if (did.startsWith('space:')) {
    return { kind: 'localspace', display: did.slice(6) };
  }
  if (did.includes('|')) {
    const [arb, key] = did.split('|', 2);
    return { kind: 'remotespace', display: `${arb}/${key}` };
  }
  return { kind: 'user', display: did };
}

// ---------------------------------------------------------------------------
// ID generation
// ---------------------------------------------------------------------------

let arbiterIdCounter = 0;
export function generateArbiterDid(): string {
  arbiterIdCounter++;
  return `arbiter${arbiterIdCounter}`;
}

export function generateUserId(label: string): string {
  return label.toLowerCase().replace(/\s+/g, '-');
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
  let hash = 0;
  for (let i = 0; i < label.length; i++) {
    hash = label.charCodeAt(i) + ((hash << 5) - hash);
  }
  const h = Math.abs(hash % 360);
  return `hsl(${h}, 45%, 60%)`;
}
