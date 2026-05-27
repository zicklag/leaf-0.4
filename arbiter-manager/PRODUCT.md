# Product

## Register

product

## Users

Power users managing organization / community / group accounts on the Arbiter authorization system. They are technically capable — familiar with DIDs, XRPC, and access control concepts — but their primary job is running their org's auth, not operating the server itself.

They come to the manager app in two modes:

- **Quick task mode:** adding a member, checking who has access to a space, toggling a config. They need the fastest path to the action.
- **Deep config mode:** editing policies, reviewing resolved member lists, restructuring delegation chains across spaces. They need full visibility and control.

The app must not confuse quick-mode users or block deep-mode users. Every capability is available; the interface reveals complexity progressively.

## Product Purpose

Provide complete, browser-based management for Arbiter-based authorization. Users authenticate with their ATProto account, connect to any arbiter they control, and get a full dashboard — spaces, members, policies, config — with nothing held back.

Success means a power user never needs to open a terminal or curl an endpoint to manage their org's authorization model. Everything the arbiter server supports, the manager exposes — from adding a member to editing Rego policies — in a single, logged-in interface.

## Brand Personality

Precise, direct, capable. Shares DNA with the simulator (warmth, clarity, economy of means) but leans slightly more utilitarian — this is a tool for getting work done, not a teaching sandbox.

Three words: *precise, direct, capable.*

- **Precise** — Data is always accurate, labels are unambiguous, actions have clear effects.
- **Direct** — No decorative fluff. Every element earns its place by helping the user do something.
- **Capable** — Advanced features are not hidden. They're surfaced when the user is ready for them, not locked behind secret panels.

The tone is respectful of the user's expertise. No hand-holding, no jargon-dropping. Information presented cleanly; the user decides what to do with it.

## Anti-references

- Overdesigned admin panels with heavy gradients, glassmorphism, or decorative blurs
- Generic SaaS dashboards with hero metrics, card grids, and "analytics" widgets that nobody reads
- Terminal-native dark mode aesthetics that sacrifice readability for hacker cred
- "No-code" UX patterns that treat the user as a beginner (wizards, step-by-step tours, chat-first interfaces)
- Nested card patterns and excessive container nesting

## Design Principles

- **Full control, progressive revelation.** Every feature is accessible, but the default view shows only what a quick-task user needs. Advanced features (policies, delegation chains, config JSON) are one deliberate click away.
- **The arbiter model IS the interface.** Spaces, members, access levels, delegations — the conceptual model maps directly to UI structure. No abstraction layers that obscure what's happening.
- **State is truth.** Loading, empty, error, and success states are designed, not afterthoughts. Every mutation shows its result immediately, and every error from the server is surfaced clearly.
- **Respect the user's context.** The app remembers previously managed arbiters, restores session state, and surfaces the most relevant information first. The user should never re-enter what they've already told the app.
- **Economy of means.** System fonts, restrained color, minimal chrome. The interface disappears into the task.

## Accessibility & Inclusion

- WCAG AA contrast minimums
- Works without animations (content loaded, motion as enhancement only)
- Keyboard-navigable throughout (tab stops, focus indicators, no keyboard traps)
- Color-independent indicators alongside any color-coded states (access levels, error/success)
- Respect `prefers-reduced-motion`
