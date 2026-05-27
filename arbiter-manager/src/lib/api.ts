import type {
  Did,
  SpaceKey,
  SpaceSummary,
  MemberEntry,
  ResolvedMemberEntry,
  MissingSpaceEntry,
  MemberUnion,
} from './types';
import { NSID } from './types';

/**
 * XRPC client for the arbiter server.
 * Every request goes through the user's PDS via the atproto-proxy header,
 * which handles JWT auth transparently.
 */
export class ArbiterClient {
  private pdsUrl: string;
  private accessJwt: string;

  constructor(pdsUrl: string, accessJwt: string) {
    this.pdsUrl = pdsUrl.replace(/\/+$/, '');
    this.accessJwt = accessJwt;
  }

  private get authHeaders(): Record<string, string> {
    return {
      Authorization: `Bearer ${this.accessJwt}`,
    };
  }

  private xrpcUrl(nsid: string): string {
    return `${this.pdsUrl}/xrpc/${nsid}`;
  }

  /** Generic XRPC query (GET) */
  private async query<T>(
    nsid: string,
    params: Record<string, string | number | undefined>,
  ): Promise<T> {
    const searchParams = new URLSearchParams();
    for (const [key, val] of Object.entries(params)) {
      if (val !== undefined) searchParams.set(key, String(val));
    }
    const url = `${this.xrpcUrl(nsid)}?${searchParams.toString()}`;
    const res = await fetch(url, { headers: this.authHeaders });
    if (!res.ok) {
      const body = await res.json().catch(() => ({}));
      throw new XrpcRequestError(res.status, nsid, body.error, body.message);
    }
    return res.json();
  }

  /** Generic XRPC procedure (POST) */
  private async procedure<T>(nsid: string, body: Record<string, unknown>): Promise<T> {
    const res = await fetch(this.xrpcUrl(nsid), {
      method: 'POST',
      headers: { ...this.authHeaders, 'Content-Type': 'application/json' },
      body: JSON.stringify(body),
    });
    if (!res.ok) {
      const body = await res.json().catch(() => ({}));
      throw new XrpcRequestError(res.status, nsid, body.error, body.message);
    }
    // Procedures may return empty 200
    if (res.status === 200) {
      const text = await res.text();
      return text ? JSON.parse(text) : (undefined as T);
    }
    return res.json();
  }

  // -- Arbiters --

  async createArbiter(arbiterDid: Did, config: Record<string, unknown>) {
    return this.procedure(NSID.createArbiter, { arbiterDid, config });
  }

  async deleteArbiter(arbiterDid: Did, resolverDepth?: number) {
    return this.procedure(NSID.deleteArbiter, { arbiterDid, resolverDepth });
  }

  async getArbiterConfig(arbiterDid: Did) {
    return this.query<Record<string, unknown>>(NSID.getArbiterConfig, { arbiterDid });
  }

  async setArbiterConfig(arbiterDid: Did, config: Record<string, unknown>) {
    return this.procedure(NSID.setArbiterConfig, { arbiterDid, config });
  }

  // -- Spaces --

  async listSpaces(arbiterDid: Did) {
    return this.query<{ spaces: SpaceSummary[] }>(NSID.listSpaces, { arbiterDid });
  }

  async createSpace(
    arbiterDid: Did,
    spaceKey: SpaceKey,
    spaceType: string,
    config: Record<string, unknown>,
  ) {
    return this.procedure(NSID.createSpace, { arbiterDid, spaceKey, spaceType, config });
  }

  async getSpaceConfig(arbiterDid: Did, spaceKey: SpaceKey) {
    return this.query<{ spaceType: string; config: Record<string, unknown> }>(NSID.getSpaceConfig, {
      arbiterDid,
      spaceKey,
    });
  }

  async setSpaceConfig(
    arbiterDid: Did,
    spaceKey: SpaceKey,
    spaceType: string,
    config: Record<string, unknown>,
  ) {
    return this.procedure(NSID.setSpaceConfig, { arbiterDid, spaceKey, spaceType, config });
  }

  async deleteSpace(arbiterDid: Did, spaceKey: SpaceKey) {
    return this.procedure(NSID.deleteSpace, { arbiterDid, spaceKey });
  }

  // -- Members --

  async getSpaceMembers(arbiterDid: Did, spaceKey: SpaceKey) {
    return this.query<{ members: MemberEntry[] }>(NSID.getSpaceMembers, { arbiterDid, spaceKey });
  }

  async resolveSpaceMembers(arbiterDid: Did, spaceKey: SpaceKey, resolverDepth?: number) {
    return this.query<{ members: ResolvedMemberEntry[]; missingSpaces: MissingSpaceEntry[] }>(
      NSID.resolveSpaceMembers,
      { arbiterDid, spaceKey, resolverDepth },
    );
  }

  async setSpaceMemberAccess(
    arbiterDid: Did,
    spaceKey: SpaceKey,
    member: MemberUnion,
    access: Record<string, unknown>,
  ) {
    return this.procedure(NSID.setSpaceMemberAccess, {
      arbiterDid,
      spaceKey,
      member,
      access,
    });
  }

  async removeSpaceMember(arbiterDid: Did, spaceKey: SpaceKey, member: MemberUnion) {
    return this.procedure(NSID.removeSpaceMember, {
      arbiterDid,
      spaceKey,
      member,
      access: {},
    });
  }
}

export class XrpcRequestError extends Error {
  status: number;
  nsid: string;
  errorCode?: string;

  constructor(status: number, nsid: string, errorCode?: string, message?: string) {
    super(message || errorCode || `XRPC error ${status}`);
    this.name = 'XrpcRequestError';
    this.status = status;
    this.nsid = nsid;
    this.errorCode = errorCode;
  }

  get isPermissionDenied(): boolean {
    return this.errorCode === 'ErrPermissionDenied';
  }

  get isArbiterNotExists(): boolean {
    return this.errorCode === 'ErrArbiterNotExists';
  }

  get isSpaceNotExists(): boolean {
    return this.errorCode === 'ErrSpaceNotExists';
  }
}
