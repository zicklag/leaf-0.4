// ---------------------------------------------------------------------------
// Identifiers
// ---------------------------------------------------------------------------

export type Did = string;
export type Handle = string;
export type SpaceKey = string;

// ---------------------------------------------------------------------------
// XRPC method identifiers
// ---------------------------------------------------------------------------

export const NSID = {
  createArbiter: 'town.muni.arbiter.createArbiter' as const,
  getArbiterConfig: 'town.muni.arbiter.getArbiterConfig' as const,
  setArbiterConfig: 'town.muni.arbiter.setArbiterConfig' as const,
  deleteArbiter: 'town.muni.arbiter.deleteArbiter' as const,
  createSpace: 'town.muni.arbiter.createSpace' as const,
  getSpaceConfig: 'town.muni.arbiter.getSpaceConfig' as const,
  setSpaceConfig: 'town.muni.arbiter.setSpaceConfig' as const,
  deleteSpace: 'town.muni.arbiter.deleteSpace' as const,
  listSpaces: 'town.muni.arbiter.listSpaces' as const,
  getSpaceMembers: 'town.muni.arbiter.getSpaceMembers' as const,
  resolveSpaceMembers: 'town.muni.arbiter.resolveSpaceMembers' as const,
  setSpaceMemberAccess: 'town.muni.arbiter.setSpaceMemberAccess' as const,
  removeSpaceMember: 'town.muni.arbiter.removeSpaceMember' as const,
} as const;

export type Nsid = (typeof NSID)[keyof typeof NSID];

// ---------------------------------------------------------------------------
// API types matching the lexicons
// ---------------------------------------------------------------------------

export interface MemberDid {
  did: Did;
}

export interface MemberLocalSpace {
  spaceKey: string;
}

export interface MemberRemoteSpace {
  arbiterDid: Did;
  spaceKey: string;
}

export type MemberUnion =
  | { $type: 'town.muni.arbiter.defs#memberDid'; did: string }
  | { $type: 'town.muni.arbiter.defs#memberLocalSpace'; spaceKey: string }
  | { $type: 'town.muni.arbiter.defs#memberRemoteSpace'; arbiterDid: string; spaceKey: string };

export interface MemberEntry {
  member: MemberUnion;
  access: Record<string, unknown>;
}

export interface ResolvedMemberEntry {
  did: Did;
  access: Record<string, unknown>;
}

export interface MissingSpaceEntry {
  arbiterDid: Did;
  spaceKey: string;
  access: Record<string, unknown>;
}

export interface SpaceSummary {
  key: SpaceKey;
  spaceType: string;
  config?: Record<string, unknown>;
}

export interface XrpcError {
  error: string;
  message?: string;
}

// ---------------------------------------------------------------------------
// Access levels
// ---------------------------------------------------------------------------

export const ALL_ACCESSES = [
  'ReadMemberList',
  'IsMember',
  'AddMembers',
  'RemoveMembers',
  'ConfigureSpace',
  'CreateSpaces',
  'RemoveSpace',
  'Owner',
] as const;

export type Access = (typeof ALL_ACCESSES)[number];

export const ACCESS_LABELS: Record<Access, string> = {
  ReadMemberList: 'Read Members',
  IsMember: 'Member',
  AddMembers: 'Add Members',
  RemoveMembers: 'Remove Members',
  ConfigureSpace: 'Configure Space',
  CreateSpaces: 'Create Spaces',
  RemoveSpace: 'Delete Spaces',
  Owner: 'Owner',
};

// ---------------------------------------------------------------------------
// Managed community (persisted in localStorage)
// ---------------------------------------------------------------------------

export interface ManagedCommunity {
  did: Did;
  label: string;
  addedAt: number;
}
