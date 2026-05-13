// Types matching the Rust view types serialized via serde-wasm-bindgen.
// Uses camelCase throughout — serde rename_all = "camelCase" handles naming.

export type Access =
  | 'ReadMemberList'
  | 'IsMember'
  | 'AddMembers'
  | 'RemoveMembers'
  | 'ConfigureSpace'
  | 'CreateSpaces'
  | 'RemoveSpace'
  | 'Owner';

export const ALL_ACCESSES: Access[] = [
  'ReadMemberList',
  'IsMember',
  'AddMembers',
  'RemoveMembers',
  'ConfigureSpace',
  'CreateSpaces',
  'RemoveSpace',
  'Owner',
];

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
// View types (returned by wasm, no JSON parsing needed)
// ---------------------------------------------------------------------------

export interface MemberEntryView {
  memberType: string;
  value: string;
  access: string;
}

export interface MissingSpaceView {
  arbiterDid: string;
  spaceKey: string;
  access: string;
}

// Discriminated union matching Rust's EffectView serde(tag = "effectType")
export type EffectView =
  | {
      effectType: 'Respond';
      reqId: number;
      ok: boolean;
      memberList: MemberEntryView[];
      missingSpaces: MissingSpaceView[];
      error: string;
    }
  | {
      effectType: 'SendMessage';
      toDid: string;
      arbiterDid: string;
      spaceKey: string;
      srcJobId: number;
      resolverDepth: number;
    }
  | {
      effectType: 'ArbiterChanged';
      arbiterDid: string;
    }
  | {
      effectType: 'ArbiterDeleted';
      arbiterDid: string;
    };

export interface SpaceConfigView {
  publicRecords: boolean;
  publicMembers: boolean;
}

export interface SpaceView {
  key: string;
  config: SpaceConfigView;
  members: MemberEntryView[];
}

export interface ArbiterView {
  did: string;
  version: number;
  spaces: SpaceView[];
}

export interface PendingJobView {
  id: number;
  userDid: string;
  spaceKey: string;
  argsType: string;
}

export interface ServerStateView {
  time: number;
  arbiters: ArbiterView[];
  pendingJobs: PendingJobView[];
}

// ---------------------------------------------------------------------------
// Message types (sent TO wasm as JSON string — complex serde enums)
// ---------------------------------------------------------------------------

export type MessageKind =
  | { type: 'createArbiter' }
  | { type: 'deleteArbiter' }
  | { type: 'fetchMembers' }
  | { type: 'createSpace' }
  | { type: 'configureSpace'; publicRecords: boolean; publicMembers: boolean }
  | { type: 'deleteSpace' }
  | { type: 'setMemberAccess'; member: { tag: string; value: string }; access: Access }
  | { type: 'removeMember'; member: { tag: string; value: string } }
  | { type: 'replyResolvedMembers'; members: { memberList: Record<string, string>; missingSpaces: Record<string, string> } };

export interface Message {
  userDid: string;
  arbiterDid: string;
  spaceKey: string;
  srcJobId: number;
  resolverDepth: number;
  kind: MessageKind;
}

// ---------------------------------------------------------------------------
// Application-level types
// ---------------------------------------------------------------------------

export interface UserAccount {
  did: string;
  label: string;
}
