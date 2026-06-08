# Adding Lexicon Definitions

If the arbiter-manager needs XRPC calls to a lexicon that isn't currently in our
`lexicons/` directory, you can install it from the AT Protocol lexicon registry:

```sh
cd lexicons
npx @atproto/lex install 'com.atproto.repo.putRecord'
```

Then regenerate the TypeScript type definitions:

```sh
cd arbiter-manager
rm -rf src/lib/lexicons
pnpm i
```

The `pnpm i` triggers a `postinstall` script that runs `@atproto/lex build`
to regenerate all the TypeScript files from the JSON lexicons.

> Note: You may also need to add the corresponding `rpc:<method>?aud=*` scope
> to the OAuth scope list in `src/lib/auth.svelte.ts`.

# OAuth Scopes

Each new XRPC method that the app calls needs its scope added to the
`atprotoOauthScope` string in `src/lib/auth.svelte.ts` so the OAuth client
requests permission when signing in.