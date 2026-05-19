# Design

A standalone HTML visualizer for `town.muni.arbiter.*` XRPC lexicons — a developer reference tool.

## Theme

Light mode. Developers use this in a browser tab alongside their editor. The interface is calm and readable in varied ambient light — a well-lit desk, a sunlit coffee shop, a dim terminal room. No dark-mode toggle at v1; light backgrounds give the highest contrast for dense schema content.

## Color Strategy

**Restrained.** Tinted neutrals with a single accent for interactive elements and state.

### Palette

```
Surface        oklch(98% 0.005 260)    /* page background, warm-tinted white */
CardBg         oklch(96% 0.006 260)    /* secondary surfaces, code blocks */
Border         oklch(88% 0.008 260)    /* dividers, subtle edges */
TextPrimary    oklch(25% 0.012 260)    /* body, headings */
TextSecondary  oklch(45% 0.015 260)    /* labels, descriptions */
TextTertiary   oklch(60% 0.012 260)    /* metadata, type badges */

Accent         oklch(50% 0.18 260)     /* links, active filters, primary interactive */
AccentDim      oklch(40% 0.15 260)     /* hover states */
AccentBg       oklch(92% 0.04 260)     /* selected items, light highlight */

QueryBadge     oklch(50% 0.14 200)     /* query endpoint badge */
ProcedureBadge oklch(50% 0.16 30)      /* procedure endpoint badge */

ErrorRed       oklch(50% 0.18 25)      /* error names in lists */
RequiredMark   oklch(50% 0.15 200)     /* required field indicator */
```

## Typography

**System font stack.** No external font loads.

```css
font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", system-ui, sans-serif;
```

Monospace for code, types, and NSIDs:

```css
font-family: ui-monospace, "SF Mono", "Cascadia Code", "JetBrains Mono", Menlo, monospace;
```

### Scale (rem)

| Step | Size  | Weight  | Use                           |
|------|-------|---------|-------------------------------|
| 0    | 0.75  | 400     | metadata, badges              |
| 1    | 0.875 | 400     | description text, table cells |
| 2    | 1     | 400     | body                          |
| 3    | 1.125 | 500     | property names, section heads |
| 4    | 1.25  | 600     | endpoint name                 |
| 5    | 1.5   | 600     | page heading                  |

Line length: 75ch max on prose sections. Tables and schema listings can run wider.

## Layout

Single-page app with two columns:

- **Sidebar (260px)**: scrollable endpoint list grouped by namespace section. Active endpoint highlighted.
- **Main content**: scrollable detail pane for the selected endpoint.

When no endpoint is selected, the main pane shows an overview: the count of queries vs procedures, a description of the namespace, and a prompt to select an endpoint.

Search bar pinned at the top of the sidebar for filtering endpoints by name.

### Spacing scale (rem)

| Token | Value | Use                           |
|-------|-------|-------------------------------|
| xs    | 0.25  | badge padding                 |
| sm    | 0.5   | table cell padding, gaps      |
| md    | 1     | card padding, section spacing |
| lg    | 1.5   | major section separation      |
| xl    | 2     | page padding                  |

## Components

### Endpoint Card (main detail)

- Header: type badge (Query/Procedure) + full NSID + endpoint name
- Description rendered as readable paragraph (handling \n line breaks)
- Sections (when present):
  - **Parameters** (queries) or **Input** (procedures): schema table with property name, type, required indicator, description, format
  - **Output**: schema table or type union display
  - **Errors**: list of error names + descriptions
- Each card is a collapsible section within the detail view (one selected at a time)

### Schema Table

| Column | Content |
|--------|---------|
| Name   | property name, monospace |
| Type   | type badge: `string`, `integer`, `union`, `array`, `object`, `ref` |
| Req.   | `*` required indicator in accent color, or empty |
| Desc.  | property description |

### Type Badges

Small inline badges in a neutral tint for type names. Union refs link to defs.

### Defs Panel

A separate section in the sidebar or a toggleable panel showing shared type definitions from `defs.json`, with cross-references from endpoints that use them.

## Motion

Minimal and purposeful:
- Sidebar item highlight fades in 150ms ease-out
- Detail content fades in 200ms ease-out on selection
- Search filtering uses instant show/hide (no animation for filtering — speed matters more than choreography)
- No page-load animations, no entrance sequences, no hover scale transforms

## Interaction States

- **Search**: real-time filter as you type, no debounce delay visible
- **Sidebar items**: default → hover (background tint) → active (accent bg + text) → selected (persistent highlight)
- **Links to defs**: inline anchor links that scroll to the def section
- **Badges**: no hover state (purely informative, not interactive)

## Responsive

At widths <720px, sidebar collapses to a top bar with a hamburger menu / slide-out panel. Main content fills full width.
