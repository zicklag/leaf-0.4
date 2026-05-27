# Design

A browser-based management interface for Arbiter authorization, built as a Svelte 5 SPA with `@foxui/core`.

## Design System

This project defers to `@foxui/core` (fox-ui) for its visual foundation. The following documents the decisions atop that foundation.

### Theme

Light/dark toggle respecting system preference via `prefers-color-scheme`. The toggle is available in the app chrome (toolbar/header). Default state follows the system; user choice is persisted.

fox-ui's `ThemeToggle` component handles this natively.

### Color Strategy

**Deferred to fox-ui.** The kit's two-color system (base + accent) provides the full palette. Default accent and base colors from fox-ui are used.

If a custom accent or base is desired later, it's set via a class on `<html>` (e.g. `class="blue zinc"`) or by overriding CSS variables.

### Typography

**Deferred to fox-ui.** The kit ships with Geist (sans-serif) as the default font. Monospace for code, NSIDs, DIDs, and policy text via Geist Mono.

### Components

The following fox-ui components form the core UI toolkit:

| Component | Usage |
|-----------|-------|
| `Button` | All actions (primary, secondary, danger, icon) |
| `Input` | Text inputs, search fields |
| `Select` | Dropdowns for access levels, space selection, config pickers |
| `Badge` | Status badges (space type, access level, connection state) |
| `Box` | Consistent container for list items, config sections |
| `Modal` | Confirmations, detail views, policy viewer |
| `Sheet` | Slide-in panels for forms, member management, config editing |
| `Tabs` | Dashboard sections (spaces, members, config, policy) |
| `ScrollArea` | Scrollable content panels |
| `Switch` | Toggle settings (public records, public members) |
| `Checkbox` | Selection in member/space lists |
| `Tooltip` | Explanations for access levels, DID format, advanced controls |
| `Sonner` / `toast` | Success/error notifications for mutations |
| `Sidebar` | Navigation, arbiter switcher, managed DIDs list |

### Layout

Multi-panel layout with three zones:

1. **Left sidebar** — Managed arbiter DID list (persisted across sessions), current user info, app-level actions
2. **Main content area** — Tabbed dashboard for the selected arbiter (spaces, members, config, policy)
3. **Optional sheet/modal** — Overlaid forms for creating/editing spaces, members, and config

Progressive disclosure: the main dashboard shows spaces and members by default. Policy editing and advanced config live behind clearly labeled tabs or buttons.

### Interaction Patterns

- **State is truth:** Every mutation (create space, add member, update config) shows loading state, then immediate success/error via toast. The relevant list auto-refreshes.
- **Empty states:** When no arbiter is selected (first login), show a prompt to enter or select a DID. When an arbiter has no spaces, show a "create your first space" prompt.
- **Confirmation:** Destructive actions (delete space, remove member) use a Modal for confirmation. Non-destructive mutations (add member, update config) execute directly.
- **Session persistence:** Managed arbiter DIDs, selected arbiter, and theme preference are persisted.
