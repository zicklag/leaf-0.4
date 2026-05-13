# Product

## Register

product

## Users

Developers, architects, and technical decision-makers exploring how to structure
authorization with the Arbiter. They're in learning/experimentation mode — trying out
delegation patterns, verifying access compositions, building intuition. They might be
in a browser tab between code, in a workshop, or sharing a scenario with a colleague.

The job to be done: understand what the arbiter can do and how it works, by
manipulating configuration directly and seeing real computed results.

## Product Purpose

An interactive web simulator that teaches the Arbiter authorization model through
direct manipulation. Users create virtual accounts, arbiters, and spaces, set up
delegation chains, and see the resulting member lists and access levels computed by
the real arbiter state machine — all in the browser, with no server needed.

Success means someone can go from "what's an arbiter?" to "I can design the auth
structure for my app" in a single session.

## Brand Personality

- **Precise** — The arbiter is formally specified and rigorously implemented; the
  tool reflects that accuracy in its data display and error reporting.
- **Approachable** — This is a teaching tool, not a production admin panel. It
  should feel welcoming, not intimidating. Friendly without being childish.
- **Warm-confident** — Linear-level polish and restraint, but warmer. Less austere,
  more human. Think "well-designed kitchen tool" not "laboratory instrument."

Three words: precise, approachable, warm-confident.

## Anti-references

- **No corporate/SaaS-cream** — No white + blurple gradients, no hero metrics, no
  generic dashboard aesthetic.
- **No neon-on-black terminal aesthetic** — The tool is about clarity, not hacker
  cred.
- **Not Linear-level austere** — Less cold. A touch more personality in color,
  illustration, and copy. Linear's polish, but lighter-hearted.
- **Not gamified/playful** — No confetti, no cartoon mascots, no "you leveled up"
  nonsense. Fun means satisfying interactions, not gamification.

## Design Principles

1. **Show the mechanism.** Delegation chains, resolution paths, and access
   composition should be visible and traceable. Don't hide the workings — that's
   the whole point.

2. **Safe exploration.** Every action is reversible. No consequences for trying
   things. Reset is always one click away.

3. **Precision with personality.** Accurate data display, but the surrounding
   chrome should feel warm and human. Error messages should teach, not scold.

4. **Progressive revelation.** Start simple — one arbiter, one space. Grow as the
   user explores. Don't overwhelm with options on first load.

5. **Warm minimalism.** Less chrome, more content. What remains should feel
   intentional and crafted, not stripped bare.

## Accessibility & Inclusion

- WCAG 2.1 AA for color contrast
- Keyboard navigable canvas and forms
- Respect `prefers-reduced-motion`
- Avoid color-only distinctions in access level indicators (pair with icons/text)
