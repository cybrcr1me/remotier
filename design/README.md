# Handoff: Remotier brand identity & design language

## Overview
Brand identity and UI design language for **Remotier**, a desktop SSH session manager (hosts, keychain, identities, known hosts) for developers, sysadmins and SRE teams. This bundle defines the logo, palette, typography, spacing, motion, iconography, voice and component treatments the app and marketing site should be built from.

## About the design files
The files here are **design references created in HTML** — a prototype of the brand system showing intended look and behaviour, not production code to copy. The job is to recreate the treatments in the target codebase's own environment (React, Electron, SwiftUI, Tauri, whatever the app already uses) with its established patterns. If no UI environment exists yet, pick the framework that fits the desktop app and implement there.

The design tokens, on the other hand, **are** meant to be adopted directly: `tokens.css` and `tokens.json` are the source of truth for values.

## Fidelity
**High fidelity.** Colours, type, spacing, radii and motion values are final. Layout of the app screens themselves is not final — section 12 of the reference page shows the design language applied to the existing shell (from the screenshot supplied by the product owner), not a finished screen spec.

## Start here
1. `BRAND.md` — the full identity: positioning, values, personality, tagline, voice rules, logo construction, icon spec, design language.
2. `tokens.css` / `tokens.json` — every value, named. Drop `tokens.css` into the app and reference `var(--rm-*)`.
3. `CLAUDE.md.snippet` — paste into the repo's `CLAUDE.md` so Claude Code applies the language on every task without re-reading this bundle.
4. `assets/` — logo lockup, wordmark, app icon and favicon as SVG.
5. `Remotier Brand Identity.dc.html` — the visual reference page. Open in a browser.

## Fonts
Both are free and on Google Fonts. Self-host in the app rather than fetching at runtime:
- **Martian Mono** — display, weights 600 and 700.
- **JetBrains Mono** — interface and body, weights 300, 400, 500.

## Design tokens
See `tokens.css` (CSS custom properties) and `tokens.json` (same values as data, for a token pipeline or a native theme file). Summary: dark-only surface ramp #000 → #2A2E2A, four text greys #E8EBE6 → #6B6F6B, single accent #D9FF00, three status hues, 8px space grid, radii 0/5/10, 120ms and 240ms motion on `cubic-bezier(0.2, 0, 0, 1)`.

## Component treatments
| Component | Spec |
| --- | --- |
| Primary button | `--rm-lime` fill, `#000` text, 12px/500, padding 11px 20px, radius 5px; hover `--rm-lime-hover` |
| Secondary button | 1px `--rm-control` border, `--rm-bone` text, padding 10px 20px, radius 5px; hover border `--rm-dim` |
| Tertiary / cancel | `--rm-dim` text, no border, no fill; hover `--rm-bone` |
| Text input | `#000` fill, 1px `--rm-control`, radius 5px, padding 10px 12px, lime `$` prefix, lime 7×14px block cursor |
| Sidebar item | 12px, `--rm-ink-2`, radius 5px, padding 8px 10px, shortcut key right-aligned in `--rm-dim` 10px; hover `--rm-raised` + `--rm-bone` |
| Data row | 36px, radius 0, 1px `--rm-rule-lo` bottom border, 6px status square, name in `--rm-bone`, address in `--rm-ink-2`, meta right-aligned in `--rm-dim`; hover `#070807` |
| Panel | `--rm-shell` fill, 1px `--rm-rule`, radius 10px, no shadow |
| Empty state | Martian Mono 600 13px statement + `--rm-dim` 12px line naming the next keystroke |

## Assets
- `assets/logo-lockup.svg` — mark + wordmark, horizontal.
- `assets/wordmark.svg` — wordmark with cursor.
- `assets/mark.svg` — lime square with `>_`.
- `assets/app-icon.svg` — 1024 dock icon, squircle tile.
- `assets/favicon.svg` — 16px lime square with `>`.

**These SVGs reference the Martian Mono font by name.** They render correctly only where that font is installed. Before shipping — app bundle, store listing, website — open each in a vector editor and convert the text to outlined paths, or have a designer redraw the wordmark once and replace these files.

## Not included / open items
- Outlined vector logo files (see above).
- `.icns` / `.ico` / PNG icon exports at the platform sizes.
- A real product screenshot for the marketing header (the reference page has a striped placeholder marked "product shot").
- Marketing site pages beyond the two header treatments.

## Files
| File | What it is |
| --- | --- |
| `BRAND.md` | Identity and design-language spec, self-contained |
| `tokens.css` | CSS custom properties — adopt directly |
| `tokens.json` | Same values as structured data |
| `CLAUDE.md.snippet` | Paste-ready block for the repo's CLAUDE.md |
| `assets/*.svg` | Logo, mark, app icon, favicon |
| `Remotier Brand Identity.dc.html` | Visual reference page (open in a browser) |
