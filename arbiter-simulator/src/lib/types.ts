// Re-exports from arbiter-wasm + UI-only helpers.
export type {
  Access,
  Member,
  Message,
  MessageKind,
  EffectView,
  ServerStateView,
  ArbiterView,
  SpaceView,
  SpaceConfigView,
  MemberEntryView,
  MissingSpaceView,
  PendingJobView,
  ResolvedMemberList,
  SpaceId,
} from 'arbiter-wasm';

// UI helpers (not in wasm)
import type { Access } from 'arbiter-wasm';

export const ALL_ACCESSES: Access[] = [
  'ReadMemberList', 'IsMember', 'AddMembers', 'RemoveMembers',
  'ConfigureSpace', 'CreateSpaces', 'RemoveSpace', 'Owner',
];

export const ACCESS_LABELS: Record<Access, string> = {
  ReadMemberList: 'Read Members', IsMember: 'Member',
  AddMembers: 'Add Members', RemoveMembers: 'Remove Members',
  ConfigureSpace: 'Configure Space', CreateSpaces: 'Create Spaces',
  RemoveSpace: 'Delete Spaces', Owner: 'Owner',
};

export interface UserAccount {
  did: string;
  label: string;
}
