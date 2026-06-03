import {
  CompositeDidDocumentResolver,
  LocalActorResolver,
  PlcDidDocumentResolver,
  WebDidDocumentResolver,
} from '@atcute/identity-resolver';

export const didResolver = new CompositeDidDocumentResolver({
  methods: {
    plc: new PlcDidDocumentResolver(),
    web: new WebDidDocumentResolver(),
  },
});

export const actorResolver = new LocalActorResolver({
  handleResolver: {
    async resolve(handle) {
      const resp = await fetch(
        `https://resolver.roomy.chat/xrpc/com.atproto.identity.resolveHandle?handle=${encodeURIComponent(handle)}`,
      );
      const { did } = await resp.json();
      return did;
    },
  },
  didDocumentResolver: didResolver,
});
