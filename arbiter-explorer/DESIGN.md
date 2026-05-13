# Design

## Theme

Light mode. Warm off-white base, tinted slightly toward amber for approachability.
Dark mode available via `prefers-color-scheme` but not default.

The tool is used during the day, alongside code editors. It should feel like a
well-lit desk, not a control room.

## Color

Strategy: **Restrained** with warm personality. One warm amber accent carries
primary actions, selection, and state indicators. Neutrals are tinted toward
the accent at low chroma.

| Token | Light | Dark | Usage |
|-------|-------|------|-------|
| `--bg-base` | oklch(0.97 0.005 70) | oklch(0.15 0.008 70) | Page background |
| `--bg-surface` | oklch(0.99 0.002 70) | oklch(0.19 0.006 70) | Cards, panels, sidebar |
| `--bg-raised` | oklch(0.995 0 0) | oklch(0.23 0.004 70) | Modals, dropdowns |
| `--text-primary` | oklch(0.18 0.008 70) | oklch(0.88 0.005 70) | Body, headings |
| `--text-secondary` | oklch(0.45 0.01 70) | oklch(0.55 0.01 70) | Labels, captions |
| `--text-muted` | oklch(0.65 0.005 70) | oklch(0.38 0.008 70) | Placeholders, disabled |
| `--border` | oklch(0.85 0.01 70) | oklch(0.28 0.01 70) | Separators, outlines |
| `--accent` | oklch(0.58 0.18 65) | oklch(0.62 0.18 65) | Primary actions, focus, selection |
| `--accent-hover` | oklch(0.53 0.20 65) | oklch(0.67 0.18 65) | Hover state |
| `--accent-subtle` | oklch(0.92 0.04 65) | oklch(0.22 0.06 65) | Selected bg, accent backgrounds |

Semantic colors (success/warning/error) are desaturated versions to avoid
competing with the amber accent.

## Typography

System font stack — `-apple-system, BlinkMacSystemFont, "Segoe UI", system-ui,
sans-serif` — for native feel and zero latency.

- Body: 14px / 1.5 line-height
- Small labels: 12px / 1.4
- Headings: 18px, 16px, 14px (1.125 ratio between steps)
- Monospace: `"SF Mono", "Fira Code", "Cascadia Code", monospace` for DIDs,
  access level names, and member entries

One family for everything. No display/body pairing.

## Spacing

4px base unit. Rhythmic but restrained:
- Tight (compact data): 4px, 8px
- Default: 12px, 16px
- Generous (section breaks): 24px, 32px
- Sidebar width: 280px
- Detail panel width: 320px

## Elevation

Subtle. No heavy shadows. Box shadows use the accent tint at low opacity.

| Token | Value | Usage |
|-------|-------|-------|
| `--shadow-sm` | `0 1px 2px oklch(0.18 0.008 70 / 0.04)` | Cards in surface |
| `--shadow-md` | `0 2px 8px oklch(0.18 0.008 70 / 0.06)` | Dropdowns, raised panels |
| `--shadow-lg` | `0 4px 16px oklch(0.18 0.008 70 / 0.08)` | Modals |

## Motion

- 150ms for micro-interactions (hover, focus, toggle)
- 200ms for state changes (expand/collapse, panel open)
- Ease-out-quart (`cubic-bezier(0.25, 1, 0.5, 1)`) for everything
- No orchestrated page-load sequences
- Respect `prefers-reduced-motion` — disable all transitions

## Components

### Canvas

- Arbiter nodes: rounded rectangles (12px radius) with warm border, subtle surface bg
- Space nodes: smaller cards (8px radius) nested inside arbiter containers
- Delegation edges: SVG path arrows with amber stroke, dashed for pending resolution
- $admin spaces: amber-tinted border to distinguish them
- Selected nodes: amber ring, 2px

### Access Levels

Visualized as a segmented bar or stacked indicator, not a bare enum string.
Warmest/most saturated at Owner, lightest/most muted at ReadMemberList.
Paired with a short text label for accessibility.

### Sidebar

- User list with avatars (generated initials, warm amber bg)
- Active user highlighted with accent border
- Action buttons follow standard product patterns (consistent shape, clear labels)

### Notifications

- Slide-in from top-right, 200ms ease-out-quart
- Amber-tinted bg for success, warm desaturated red for errors
