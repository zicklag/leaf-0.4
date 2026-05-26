// ---------------------------------------------------------------------------
// ID types
// ---------------------------------------------------------------------------

export type Did = string;
export type SpaceKey = string;

// ---------------------------------------------------------------------------
// Arbiter / Space / Member state
// ---------------------------------------------------------------------------

export interface MemberEntry {
  did: Did;
  access: Record<string, unknown>;
}

export interface Space {
  key: SpaceKey;
  spaceType: string;
  config: Record<string, unknown>;
  members: MemberEntry[];
}

export interface ArbiterState {
  did: Did;
  version: number;
  config: Record<string, unknown>;
  policy: string;
  spaces: Map<SpaceKey, Space>;
}

/** Serialisable snapshot of the full server state. */
export interface ServerSnapshot {
  arbiters: ArbiterSnapshot[];
}

export interface ArbiterSnapshot {
  did: Did;
  version: number;
  config: Record<string, unknown>;
  policy: string;
  spaces: SpaceSnapshot[];
}

export interface SpaceSnapshot {
  key: SpaceKey;
  spaceType: string;
  config: Record<string, unknown>;
  members: MemberEntry[];
}

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

/** Which XRPC method type each NSID is. */
export function nsidType(nsid: string): 'query' | 'procedure' {
  switch (nsid) {
    case NSID.getArbiterConfig:
    case NSID.getSpaceConfig:
    case NSID.listSpaces:
    case NSID.getSpaceMembers:
    case NSID.resolveSpaceMembers:
      return 'query';
    default:
      return 'procedure';
  }
}

// ---------------------------------------------------------------------------
// Operation result returned to the UI
// ---------------------------------------------------------------------------

export interface OpOk {
  status: 'ok';
  value?: unknown;
  members?: MemberEntry[];
  missingSpaces?: SpaceRef[];
  spaces?: SpaceSummary[];
  config?: Record<string, unknown>;
}

export interface OpError {
  status: 'error';
  error: string;
}

export type OpResult = OpOk | OpError;

export interface SpaceRef {
  arbiterDid: Did;
  spaceKey: SpaceKey;
}

export interface SpaceSummary {
  key: SpaceKey;
  spaceType: string;
}

// ---------------------------------------------------------------------------
// User accounts (UI-only)
// ---------------------------------------------------------------------------

export interface UserAccount {
  did: string;
  label: string;
}

export interface PolicyCheckLog {
  /** Description of each resolution step taken during policy evaluation. */
  steps: string[];
  /** The final value the policy returned. */
  result: unknown;
  /** Whether the policy allowed the operation. */
  allowed: boolean;
}

// ---------------------------------------------------------------------------
// Access level helpers (shared with UI)
// ---------------------------------------------------------------------------

export const ALL_ACCESSES = [
  'ReadMemberList', 'IsMember', 'AddMembers', 'RemoveMembers',
  'ConfigureSpace', 'CreateSpaces', 'RemoveSpace', 'Owner',
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
