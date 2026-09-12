# Remotier — Brand Identity v1.0

SSH session manager for developers, sysadmins and SRE teams. macOS, Windows, Linux.

## Positioning
Remotier keeps every host you own one keystroke away.

**Problem.** Credentials live in seven places. Hosts get retyped from memory. Config files drift between the laptop and the work machine. Every reconnect is small friction paid over and over.

**Position.** A single, quiet place for hosts, keys and identities, built for people who already know what SSH does. No onboarding tour. No feature carousel. It opens, it connects, it stays out of the way.

## Tagline
- **Primary:** SSH without the ceremony.
- Functional: Every host, one keystroke away.
- Provocative: You already know the command.
- Short form: Hosts, handled.
- Descriptor: SSH sessions, keys and identities in one place.

Use the primary in headers and the store listing. Use the descriptor wherever the reader has not been told what the product is yet. Never stack two of them.

## Core values
| Value | Meaning | In practice |
| --- | --- | --- |
| Terse by default | Nothing on screen the operator did not ask for. Density over decoration. | No modals for one-line decisions |
| Trust is the product | Keys and secrets are handled like they belong to someone else, because they do. | Say where a secret is stored, always |
| Measured in keystrokes | Speed is a design constraint, not a marketing claim. | The mouse is optional, never required |
| Built by operators | Decisions are settled by people who hold a pager. | Ship what we use on Monday |

## Personality
Archetype: **The Operator.** Register: dry, exact, unhurried.

Axes (marker sits near the left end of each): Terse ← → Chatty · Exact ← → Approximate · Assured ← → Eager · Deadpan ← → Playful.

**We are:** assured, deadpan, exact, unimpressed, fast.
**We are not:** eager, cute, approximate, impressed, loud.

The tone is a senior engineer answering a question they have answered before: correct, brief, faintly amused. Confidence comes from precision, never from volume. If a sentence could carry an exclamation mark, rewrite the sentence.

## Voice — UI copy rules
No exclamation marks. No emoji. No "Oops". No "Let's". No first-person plural enthusiasm. State the fact, then the next keystroke.

| Context | Write | Not |
| --- | --- | --- |
| Empty state | No host selected. Pick one. ⌘K searches all 41. | Let's get you connected! Choose a host to begin your journey. |
| Error | Host key changed. Someone changed the server, or someone is pretending to be it. | Oops! Something went wrong. Please try again later. |
| Onboarding | Import ~/.ssh/config. Nothing leaves this machine. | Welcome to Remotier! 🎉 Let us show you around. |
| Changelog | 1.2 — Ports forward on reconnect. They should have already. | We are thrilled to announce an exciting new improvement! |

## Logo
- **Wordmark:** REMOTIER in Martian Mono 700, tracking −3.5%, followed by a lime block cursor sized 0.42em × 0.72em on the baseline.
- **Mark:** `>_` set in Martian Mono 700 inside a square. Lime square / black glyph is default; outline and mono-white variants are permitted. Never on a mid grey.
- **Lockup:** mark + wordmark, gap equal to 0.35 × mark width. Default for headers, docs, decks.
- **Clear space:** one mark-width on all four sides.
- **Minimum size:** wordmark 96px wide; below that use the mark alone.
- **Misuse:** do not letterspace, stretch, condense, slant, or set the wordmark in another typeface.
- **Production note:** the wordmark must be converted to outlined vector paths before release. The SVGs in `assets/` reference the live font and are for layout reference only.

## App icon
- 1024 / 128: near-black tile `#0A0B0A`, 1px `#1F221F` border, squircle radius (29px at 128px), lime `>_` centred.
- 64: same, radius halved.
- 32 and below: tile flips to solid lime with a black glyph, for contrast against dark taskbars.
- 16: drop the underscore, keep `>`.

## Design language
- **Structure.** 1px rules instead of shadows. Panels separated by lines, not elevation. Nothing floats unless the user opened it.
- **The cursor.** A lime block cursor is the recurring device: after the wordmark, in inputs, as a list bullet, as the loading state. It replaces every spinner.
- **Density.** Rows 36px, lists flush to their container, no zebra striping. Whitespace comes from the outer margin, not between every element.
- **Icons.** 1.5px stroke, square terminals, 20px grid, no fills. Icons label navigation only, never actions that already have a shortcut.
- **Motion.** 120ms for state, 240ms for panels, `cubic-bezier(0.2, 0, 0, 1)`. No spring, no bounce. Connection states change instantly.
- **Imagery.** Only real product screenshots, cropped tight at true pixel scale. No illustrations, no 3D renders, no people at laptops.

## Accessibility floor
Body text ≥ 4.5:1 on `#000`. `--rm-dim` (#7E827E, 5.4:1) is the lowest grey allowed for text a user must read; `--rm-faint` (#6B6F6B, 4.1:1) is for captions and non-essential meta only. Never use pure #FFFFFF as text — bone reads calmer on black.
