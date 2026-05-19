# Product

## Register

product

## Users

Developers evaluating or implementing the Muni Arbiter XRPC API. They need to understand the protocol surface — what endpoints exist, what types they expect and return, and what errors they produce — before writing code against it.

## Product Purpose

Provide a complete, parse-accurate reference for the `town.muni.arbiter.*` lexicon namespace. The visualizer exists so developers can quickly grasp the API surface without reading raw JSON, spot the relationship between queries, procedures, and shared type defs, and understand error contracts for every endpoint.

## Brand Personality

Clean, focused, precise. The tone is technical but not cold — direct, helpful, and economical with words. The interface earns trust through accuracy and clarity, not decoration.

## Anti-references

- Overdesigned developer portals with heavy gradients, glassmorphism, or decorative blurs
- JSON-generator-bland outputs that just dump raw schema
- Corporate-blue dashboards with busy data tables
- "Flashy" animations that slow down understanding
- Documentation that buries the essentials in prose

## Design Principles

- **Clarity is the contract.** Every element serves one purpose: making the API surface understandable. If it doesn't help a developer answer "what does this endpoint do?", it doesn't belong.
- **Ideas first, interface second.** The types, endpoints, and error contracts are the content. The interface frames that content without competing with it.
- **Substance over flash.** Motion and visual effects earn their place only when they aid comprehension — showing relationships, revealing structure, or guiding attention.
- **Parse-accurate, not prettified.** The data comes from the actual JSON files. Nothing is hand-summarized or inferred. What the lexicons say is what the visualizer shows.
- **Economy of means.** System fonts, restrained color, minimal chrome. The tool disappears into the task.

## Accessibility & Inclusion

- WCAG AA contrast minimums
- Works without JavaScript animations (content loaded, motion as enhancement)
- Keyboard-navigable endpoint list and detail panes
- Clear color-independent indicators (required vs optional, error vs success)
