// ---------------------------------------------------------------------------
// Member — matches Rust's core::Member enum
// ---------------------------------------------------------------------------

export type MemberTag = 'MemberDid' | 'MemberLocalSpace' | 'MemberRemoteSpace';

export interface MemberDid {
  tag: 'MemberDid';
  value: string;
}

export interface MemberLocalSpace {
  tag: 'MemberLocalSpace';
  value: string;
}

export interface MemberRemoteSpace {
  tag: 'MemberRemoteSpace';
  value: { arbiterDid: string; spaceKey: string };
}

export type Member = MemberDid | MemberLocalSpace | MemberRemoteSpace;

// ---------------------------------------------------------------------------
// JobArgs — matches Rust's core::JobArgs enum
// ---------------------------------------------------------------------------

export type JobArgs =
  | { type: 'ResolveMembers' }
  | { type: 'CreateSpace'; spaceType: string; config: Record<string, unknown> }
  | { type: 'SetSpaceConfig'; spaceType: string; config: Record<string, unknown> }
  | { type: 'DeleteSpace' }
  | { type: 'SetSpaceMemberAccess'; member: Member; access: Record<string, unknown> }
  | { type: 'RemoveSpaceMember'; member: Member }
  | { type: 'DeleteArbiter' };

// ---------------------------------------------------------------------------
// Operation result — returned by SimulationEngine
// ---------------------------------------------------------------------------

export interface OperationOk {
  status: 'ok';
  members?: Array<{ did: string; access: Record<string, unknown>; via?: string }>;
  missingSpaces?: Array<{ space: { arbiterDid: string; spaceKey: string }; access: Record<string, unknown> }>;
}

export interface OperationNeedsResolution {
  status: 'needsResolution';
  jobId: number;
  spaces: Array<{ remoteArbiterDid: string; spaceKey: string }>;
}

export interface OperationDeleted {
  status: 'deleted';
}

export interface OperationError {
  status: 'error';
  error: string;
}

export type OperationResult =
  | OperationOk
  | OperationNeedsResolution
  | OperationDeleted
  | OperationError;

// ---------------------------------------------------------------------------
// Server state view — returned by getState()
// ---------------------------------------------------------------------------

export interface MemberEntryView {
  member: { tag: string; value: unknown };
  access: Record<string, unknown>;
}

export interface SpaceView {
  key: string;
  spaceType: string;
  config: Record<string, unknown>;
  members: MemberEntryView[];
}

export interface ArbiterView {
  did: string;
  version: number;
  spaces: SpaceView[];
}

export interface ServerStateView {
  time: number;
  arbiters: ArbiterView[];
}

// ---------------------------------------------------------------------------
// Access level helpers — defined locally, not from the policy
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

// ---------------------------------------------------------------------------
// User accounts (UI-only, not in engine)
// ---------------------------------------------------------------------------

export interface UserAccount {
  did: string;
  label: string;
}
