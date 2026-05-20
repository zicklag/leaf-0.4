import type {
  Member,
  Access,
  ServerStateView,
  MemberEntryView,
} from "./types";
import { ALL_ACCESSES } from "./types";

// ---------------------------------------------------------------------------
// Member building
// ---------------------------------------------------------------------------

/** Build a Member from a MemberEntryView for remove operations. */
export function buildMemberFromEntry(entry: MemberEntryView): Member | null {
  const m = entry.member;
  switch (m.tag) {
    case "MemberDid":
      return { tag: "MemberDid", value: m.value as string };
    case "MemberLocalSpace":
      return { tag: "MemberLocalSpace", value: m.value as string };
    case "MemberRemoteSpace":
      return { tag: "MemberRemoteSpace", value: m.value as { arbiterDid: string; spaceKey: string } };
    default:
      return null;
  }
}

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

/** Get a human-readable label for a member entry in the state view. */
export function memberDisplay(member: MemberEntryView): string {
  const m = member.member;
  switch (m.tag) {
    case "MemberDid":
      return `👤 ${shortDid(m.value as string)}`;
    case "MemberLocalSpace":
      return `📁 ${m.value}`;
    case "MemberRemoteSpace":
      return `🌐 ${(m.value as { arbiterDid: string }).arbiterDid}/${(m.value as { spaceKey: string }).spaceKey}`;
    default:
      return `? ${m.tag}`;
  }
}

/** Get a short label for a member type. */
export function memberTypeLabel(tag: string): string {
  switch (tag) {
    case "MemberDid": return "User";
    case "MemberLocalSpace": return "Local Space";
    case "MemberRemoteSpace": return "Remote Space";
    default: return tag;
  }
}

/** Get spaces from the server state for a given arbiter. */
export function getArbiterSpaces(state: ServerStateView, arbiterDid: string) {
  return state.arbiters.find((a) => a.did === arbiterDid)?.spaces ?? [];
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
