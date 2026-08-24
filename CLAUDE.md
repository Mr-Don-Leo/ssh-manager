# ssh-server-manaerg

You are Steve, the dedicated agent for this project.

## Instructions

-Use apple styling: https://support.apple.com/en-ae/guide/applestyleguide/welcome/web

- Recheck design, make sure to have custom drop downs, feilds, checkboxes eetc,

- Create a new branch for a new feature youre working on, and dev -> main first.

# AgentMux UI Styling Instructions

You are styling a Linux desktop app (Tauri 2 + React) called AgentMux. Follow these rules exactly.

## Core philosophy
Apple HIG-inspired, modern and restrained. Depth comes from subtle borders, soft
shadows, and translucency — never from heavy gradients or hard outlines. Whitespace
over dividers. If a native OS widget clashes with the theme, replace it with a
custom component (never rely on WebKitGTK native controls: selects, popups, etc.).

## Design tokens — the only source of truth
ALL colors, radii, fonts, and shadows come from CSS custom properties defined on
`:root` and overridden per theme. Never hard-code a color in a component. Never use
inline styles for anything a token covers. Key tokens:

- `--font-ui`: -apple-system, "SF Pro Text", "Inter", "Segoe UI", system-ui, sans-serif
- `--font-mono`: "SF Mono", ui-monospace, "JetBrains Mono", Menlo, monospace
  (Prefer SF fonts if locally installed, but NEVER bundle Apple fonts/assets — the
  license forbids redistribution. Follow the HIG's look with system fallbacks.)
- Radii: `--radius-sm: 8px`, `--radius-md: 12px`, `--radius-lg: 18px`
- Colors: `--accent` (iOS blue: #007AFF light / #0A84FF dark), `--accent-soft`
  (accent at ~12–18% alpha for fills), `--danger`, `--success`, `--bg`,
  `--bg-elevated`, `--bg-sidebar` (translucent + backdrop-filter blur),
  `--bg-input`, `--bg-hover`, `--text`, `--text-secondary`, `--text-tertiary`,
  `--border` (black/white at ~9–10% alpha)
- Shadows: `--shadow-card` (1px hairline + soft 24px ambient), `--shadow-modal`

## Theming model
Two orthogonal attributes on `<html>`:
- `data-theme="light" | "dark"` — light/dark mode (system-following by default).
  Also set `color-scheme: light/dark` so native scrollbars/widgets follow.
- `data-skin="apple" | "cyberpunk" | "xp"` — skins override ONLY tokens (plus a few
  scoped flourishes). A new skin must work by redefining tokens alone.
  - apple: as above; supports both modes.
  - cyberpunk: always dark. Deep purple-black bg (#0b0714), neon cyan accent
    (#00f0ff), magenta secondary (#ff2ec4), faint cyan grid background overlay,
    glow shadows (0 0 24px accent at low alpha), uppercase + letter-spaced
    headings, radii tightened to 4–8px.
  - xp: always light. Luna nostalgia: beige `#ece9d8` bg, blue gradient chrome
    (#3d95ff → #2456c9) on top bars, Tahoma font stack, 3–5px radii, beveled
    buttons (light gradient + 1px #7f9db9 border + inset white highlight).

## Component conventions
- Buttons: `.btn` base (8px radius, 8×16 padding, weight 500, bg `--bg-input`,
  hover `--bg-hover`, active scale(0.98)); `.btn-primary` = accent bg, white text;
  `.btn-ghost` = transparent, secondary text; `.btn-danger` = danger-colored text.
- Inputs/textarea/dropdown triggers: bg `--bg-input`, 1px `--border`, radius-sm;
  focus = accent border + 3px `--accent-soft` ring. No default outlines.
- Cards/tiles: `--bg-elevated`, 1px border, radius-lg, `--shadow-card`; hover lifts
  (translateY(-2px) + deeper shadow, 120ms ease).
- Modals: centered, radius-lg, `--shadow-modal`, behind a blurred scrim
  (rgba(0,0,0,.4) + backdrop-filter blur(4px)).
- Pills/badges: 999px radius, 11px semibold, `--accent-soft`+accent or neutral.
- Segmented controls: pill container of `--bg-input` with 3px padding; active
  segment is an elevated white/dark chip with tiny shadow (iOS style).
- Dropdowns: custom listbox — absolutely positioned panel (`--bg-elevated`, border,
  radius-sm, `--shadow-modal`), items 7×10 padding radius-6, hover `--bg-hover`,
  selected `--accent-soft` + accent + semibold. Trigger reuses input styling with
  an inline-SVG chevron (neutral #8e8e93).
- Chat bubbles: 16px radius with a 5px "tail" corner; user = accent bg white text
  right-aligned, assistant = elevated bg + border left-aligned; thinking = dashed
  border, italic, secondary color, collapsible; tool calls = compact mono cards
  with status glyph (✓/…/✕), expandable detail in `--text-secondary` mono 11.5px.
- Terminals: theme-matched xterm colors (bg matches `--terminal-bg`, cursor =
  accent, selection = accent at ~25–35% alpha).

## Typography & density
Base 14px (13px in xp). Titles 15–26px, weight 600–700, letter-spacing -0.2 to
-0.4px. Secondary metadata 11–12.5px in `--text-secondary`/`--text-tertiary`.
Mono only for code, paths, terminal, and tool payloads.

## Motion
Fast and quiet: 100–180ms ease for hover/press/appear. Entrances may slide 8px +
fade. Blinking cursor and three-dot typing bounce for streaming states. Never
animate layout-shifting properties on large surfaces.

## Accessibility & correctness
- Both modes must pass contrast on every skin; never light text on light or dark
  on dark (check custom AND native-rendered widgets).
- `user-select: none` on chrome, but text the user may want to copy (chat, code,
  terminal output) must stay selectable.
- Custom scrollbars: 9px, pill thumb in `--text-tertiary`, transparent track.
- Render agent/markdown content as DOM elements only — never raw HTML injection.