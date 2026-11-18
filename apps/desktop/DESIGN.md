---
name: Akooti Desktop
description: A local-first password vault for developers who hand secrets to agents.
colors:
  bg-app: "#0b0f14"
  bg-panel: "#111821"
  bg-elevated: "#151d28"
  bg-hover: "#1b2532"
  bg-active: "#1f2c3d"
  bg-input: "#0d141c"
  border: "#24303f"
  border-strong: "#33455c"
  text: "#e6edf3"
  text-muted: "#95a3b4"
  text-faint: "#7d8fa3"
  accent: "#4f8cf7"
  accent-strong: "#6ba1ff"
  accent-soft: "rgba(79, 140, 247, 0.14)"
  accent-contrast: "#07121f"
  success: "#33c48d"
  success-soft: "rgba(51, 196, 141, 0.14)"
  danger: "#f2626b"
  danger-soft: "rgba(242, 98, 107, 0.14)"
  warn: "#e0a63c"
  warn-soft: "rgba(224, 166, 60, 0.14)"
typography:
  display:
    fontFamily: system-ui, -apple-system, Segoe UI, Roboto, Helvetica Neue, sans-serif
    fontSize: 1.35rem
    fontWeight: 600
    lineHeight: 1.5
    letterSpacing: -0.01em
  headline:
    fontFamily: system-ui, -apple-system, Segoe UI, Roboto, Helvetica Neue, sans-serif
    fontSize: 1.15rem
    fontWeight: 600
    lineHeight: 1.5
    letterSpacing: -0.01em
  title:
    fontFamily: system-ui, -apple-system, Segoe UI, Roboto, Helvetica Neue, sans-serif
    fontSize: 0.8rem
    fontWeight: 600
    lineHeight: 1.5
    letterSpacing: 0.06em
  body:
    fontFamily: system-ui, -apple-system, Segoe UI, Roboto, Helvetica Neue, sans-serif
    fontSize: 14px
    fontWeight: 400
    lineHeight: 1.5
    letterSpacing: 0em
  label:
    fontFamily: system-ui, -apple-system, Segoe UI, Roboto, Helvetica Neue, sans-serif
    fontSize: 0.76rem
    fontWeight: 550
    lineHeight: 1.5
    letterSpacing: 0em
  mono:
    fontFamily: ui-monospace, SF Mono, JetBrains Mono, Menlo, monospace
    fontSize: 0.86em
    fontWeight: 400
    lineHeight: 1.5
    letterSpacing: 0em
rounded:
  sm: 6px
  md: 10px
  lg: 16px
  pill: 999px
spacing:
  xs: 0.35rem
  sm: 0.5rem
  md: 0.75rem
  lg: 1rem
  xl: 1.4rem
components:
  button:
    backgroundColor: "{colors.bg-elevated}"
    textColor: "{colors.text}"
    rounded: "{rounded.sm}"
    padding: 0.45rem 0.75rem
  button-hover:
    backgroundColor: "{colors.bg-hover}"
    textColor: "{colors.text}"
    rounded: "{rounded.sm}"
    padding: 0.45rem 0.75rem
  button-primary:
    backgroundColor: "{colors.accent}"
    textColor: "{colors.accent-contrast}"
    rounded: "{rounded.sm}"
    padding: 0.45rem 0.75rem
  button-primary-hover:
    backgroundColor: "{colors.accent-strong}"
    textColor: "{colors.accent-contrast}"
    rounded: "{rounded.sm}"
    padding: 0.45rem 0.75rem
  button-danger:
    backgroundColor: "{colors.danger-soft}"
    textColor: "{colors.danger}"
    rounded: "{rounded.sm}"
    padding: 0.45rem 0.75rem
  input:
    backgroundColor: "{colors.bg-input}"
    textColor: "{colors.text}"
    rounded: "{rounded.sm}"
    padding: 0.5rem 0.65rem
  chip:
    backgroundColor: "{colors.bg-elevated}"
    textColor: "{colors.text-muted}"
    rounded: "{rounded.pill}"
    padding: 2px 8px
  chip-action:
    backgroundColor: "{colors.accent-soft}"
    textColor: "{colors.accent-strong}"
    rounded: "{rounded.pill}"
    padding: 2px 8px
  card:
    backgroundColor: "{colors.bg-panel}"
    textColor: "{colors.text}"
    rounded: "{rounded.md}"
    padding: 1rem 1.1rem
  nav-item:
    backgroundColor: "{colors.bg-panel}"
    textColor: "{colors.text-muted}"
    rounded: "{rounded.sm}"
    padding: 0.45rem 0.55rem
  nav-item-active:
    backgroundColor: "{colors.bg-active}"
    textColor: "{colors.text}"
    rounded: "{rounded.sm}"
    padding: 0.45rem 0.55rem
  item-row:
    backgroundColor: "{colors.bg-panel}"
    textColor: "{colors.text}"
    rounded: "{rounded.md}"
    padding: 0.55rem
  icon-button:
    backgroundColor: "{colors.bg-panel}"
    textColor: "{colors.text-muted}"
    rounded: "{rounded.sm}"
    size: 30px
---

# Design System: Akooti Desktop

## Overview

**Creative North Star: "The Sealed Instrument"**

Akooti's interface behaves like a precision instrument in a locked case: dark machined
surfaces, a single lit indicator, and no ornament that is not doing work. Every screen is
a readout of state the user must be able to trust — which vault is open, which folder is
scoped, which secret is currently revealed — and the design earns that trust by staying
quiet. Density is a feature: this is a tool for a developer at a keyboard, so information
is packed tight and the chrome stays out of the way.

The material is flat. Depth is not implied by light falling on surfaces; it is drawn with
1px borders and a six-step tonal ramp from near-black to a lifted panel grey. A card is
not "above" the page, it is a distinct plane. The one piece of colour with authorship is
Signal Blue, and its scarcity is the point — it marks the active row, the primary action,
the focused control and a live one-time code, nothing else.

Copy is sober technical French: plain sentences, no superlatives, and an error says what
failed and what to do next. The visual system never decorates a security decision; it
surfaces it — refusal, masking, scope and expiry are all visible states rather than
modals that appear after the fact.

**Key Characteristics:**

- Dark is the canonical theme; light is a remapped token set, not a second design.
- Flat at rest: 1px borders and a tonal ramp carry all separation.
- One accent, used on ≤10% of a screen.
- 14px base, compact 0.45–0.55rem control padding, 2px inter-row gaps.
- Every state is also text, never colour alone.
- Fully keyboard-operable: a 2px accent focus ring is never removed.

## Colors

A cold, low-chroma structural palette with exactly one saturated hue.

### Primary

- **Signal Blue** (`accent`, #4f8cf7 dark / #2563eb light): the only authored colour.
  It marks the active navigation row, the primary button, link text (`accent-strong`),
  the TOTP digit group, the TOTP countdown bar, the focused control's ring, and the
  9px avatar tile behind an item's initials. In light theme it darkens so that text
  uses of it stay readable.
- **Signal Blue, Lifted** (`accent-strong`, #6ba1ff): hover state of the primary button,
  link and text-button colour, and the digits of a live code. Slightly brighter than the
  base so it reads as "live" rather than "pressed".
- **Signal Wash** (`accent-soft`, rgba blue at 14%): the selected/active fill behind
  revealed secrets, active step pills, the avatar tile, and primary-card backgrounds.
  It is a wash, never a fill you put text on.
- **Signal Ink** (`accent-contrast`, #07121f): the only text colour allowed on top of a
  solid Signal Blue fill. Documented as ≥ 4.5:1 against the accent.

### Semantic Signals

- **Verified Green** (#33c48d): success toasts, strong password cells, a positive
  count. Also appears as a soft radial wash on the unlock screen.
- **Refusal Red** (#f2626b): destructive buttons, form errors, denied audit rows, weak
  password cells. Always paired with text — "refusé", an error sentence — never a bare
  red dot.
- **Attention Amber** (#e0a63c): warnings, TOTP countdown, star/favourite icon, the
  "protection info" asterisk, and the unlock-screen vault warning banner.

### Neutral

- **Instrument Void** (#0b0f14): the app background. The darkest plane, and what the
  eye treats as "outside".
- **Panel Plane** (#111821): sidebars, the item list, cards, and modal bodies. One step
  up from the void; this is where content lives.
- **Elevated Plane** (#151d28): default button fill, chips, stats tiles, segmented
  control track, kbd keys, toast fill. Anything that sits "on" a panel.
- **Hover Plane** (#1b2532): the universal hover fill for rows, nav items, buttons and
  health rows.
- **Active Plane** (#1f2c3d): the selected row or nav item, one step further from hover.
- **Input Well** (#0d141c): text inputs and the generator preview — recessed rather than
  raised, so a field reads as an opening in the panel.
- **Hairline** (#24303f): every border, divider and card outline at rest.
- **Hairline Strong** (#33455c): borders that must hold attention — modal frame,
  active row, toast frame, hover escalation of a button.
- **Ink** (#e6edf3): body text and item titles.
- **Ink Muted** (#95a3b4): labels, secondary metadata, captions, nav rest state.
- **Ink Faint** (#7d8fa3): section headings in uppercase, hints, footnotes, counts,
  audit targets. The floor — never used for anything a user must read to act.

### Named Rules

**The One Signal Rule.** Signal Blue covers ≤10% of any screen. If a layout needs a
second accent to make sense, the layout is wrong, not the palette.

**The Semantic Variable Rule.** A component never names a colour literal; it consumes a
token. A raw hex inside a component rule is a bug, not a variation. (Two documented
exceptions carry a `var(--x, fallback)` pair: `--danger` and `--warn` in the health and
audit rows.)

**The Dark-Is-Canonical Rule.** New tokens are authored dark first. Light theme is a
key-by-key remap of the same variable names in `:root[data-theme="light"]`; a token that
only exists in one theme is incomplete.

## Typography

**Display Font:** system-ui (−apple-system, Segoe UI, Roboto, Helvetica Neue)
**Body Font:** the same system-ui stack
**Label/Mono Font:** ui-monospace (SF Mono, JetBrains Mono, Menlo)

**Character:** The system font is a deliberate non-choice: the app should look like the
platform it runs on, not like a brand. Personality comes from proportion and restraint —
a 14px body, tight negative tracking on headings, and generous uppercase letter-spacing
on section labels. Monospace is the second voice and appears only where the content is
machine truth: secrets, codes, digests, keycaps and the generator preview.

### Hierarchy

- **Display** (600, 1.35rem): the single `<h1>` of a page — the vault name, the page
  title. Negative tracking (−0.01em) keeps it from feeling loose.
- **Headline** (600, 1.15rem): modal titles and detail-pane item titles.
- **Title** (600, 0.8rem, uppercase, 0.06em): card section headings ("Identifiants",
  "Champs personnalisés"). It is a _label_, not a heading — it never carries the
  primary reading load. Sidebar headings go smaller still (0.68rem, 0.08em).
- **Body** (400, 14px, 1.5): all prose. Page intros cap at 60ch; long values inside
  `code` use `overflow-wrap: anywhere` instead of widening the column.
- **Label** (550, 0.76rem): form field names, muted by default. Buttons use 500 at
  0.82rem; chips 0.74rem; micro-labels (item metadata, hints) 0.68–0.74rem in
  `--text-faint`.
- **Values** (mono, 0.86em): secrets, TOTP seeds, URLs, digests. A live TOTP is the one
  display-grade numeric: 1.3rem, 600, 0.12em tracking, in `--accent-strong`.

### Named Rules

**The Uppercase Is Structure Rule.** Text-transform: uppercase with positive tracking is
reserved for structure labels — section headings, step names, type markers, stat keys.
Never uppercase a sentence, an error, or a button label.

**The Masked Is Muted Rule.** A hidden secret is rendered as muted, letter-spaced dots in
`--text-muted`; a revealed secret switches to full `--text` on a Signal Wash, zero
letter-spacing. The visual difference between masked and revealed must be legible at a
glance without reading the characters.

## Layout

A three-column instrument: **248px** sidebar rail, **340px** item list, and a fluid
detail pane, all inside `height: 100vh` with independent scrolling (`overflow: hidden` on
the shell, each rail scrolls itself). The vault never scrolls as a page.

Maintenance surfaces (health, import, routing, AI access, sync, backups) drop the list
column: `.content:has(> .page)` collapses to a single `minmax(0, 1fr)` column and lets
`.page` scroll, capped at **900px** wide with 1.4rem/1.6rem padding. The cap is what keeps
forms and long explanations readable on a wide monitor.

Inside a pane, multi-column content is always `repeat(auto-fit, minmax(X, 1fr))` rather
than fixed breakpoints — detail cards at 240px, form rows at 190px, checkbox grids at
170px, stat tiles at 140px. There is exactly one explicit media query in the system, at
680px, and it only restacks the onboarding step row.

The rhythm is an observed cluster, not a declared scale: **2px** between list rows and
inside segmented controls, **0.35rem** inside chips and button groups, **0.5rem** for
control padding and stacked items, **0.6–0.75rem** between form fields and card internals,
**0.9rem** for rail padding, **1.0–1.4rem** between sections, **1.6–2.5rem** for page
padding. Density is deliberate: 0.45rem vertical padding on nav items and buttons,
0.55rem on item rows.

No mobile shell exists. Below roughly 900px the three columns are cramped; the app is a
desktop product and the extension, not the desktop UI, is the small-surface answer.

## Elevation & Depth

The system is **flat with tonal layering** — not "flat design" as a slogan, but a rule
that light does not fall on these surfaces. Depth is carried by three things and nothing
else: the six-step background ramp (void → panel → elevated → hover → active → input
well), a 1px hairline on every plane boundary, and a very small set of shadows.

`--shadow-panel` is not really a shadow: it is a **1px top highlight** (`0 1px 0
rgba(255,255,255,0.03)`) that gives a card or a segmented-control thumb a lit top edge.
It appears on cards, detail cards and toasts. `--shadow-modal` (`0 24px 64px` at 55%
black) is reserved for things that genuinely leave the plane: modals, the unlock panel,
toasts.

The modal backdrop is `rgba(4,8,13,0.62)` + `backdrop-filter: blur(2px)` — the only blur
in the system, and deliberately slight, so the vault behind stays recognisable but
unreadable.

### Shadow Vocabulary

- **Hairline highlight** (`--shadow-panel`: `0 1px 0 rgba(255,255,255,0.03)`): cards,
  detail cards, toasts, the active segment. A lit edge, not a cast shadow.
- **Overlay lift** (`--shadow-modal`: `0 24px 64px rgba(0,0,0,0.55)`): modals, unlock
  panel, toasts. Anything above the shell.

### Named Rules

**The Flat-By-Default Rule.** Surfaces are flat at rest. Nothing gains elevation on
hover — hover is a background shift on the tonal ramp. If you reach for a shadow to
separate two elements, use a hairline instead.

**The Two-Shadow Rule.** The system owns exactly two shadow values. A third means
someone mistook a border for a shadow.

## Shapes

Three radii, each with one job:

- **6px** (`--radius-sm`) — everything you touch: buttons, inputs, selects, textareas,
  nav items, small lists, fieldsets, stats tiles, rules.
- **10px** (`--radius`) — everything that groups: cards, detail cards, item rows,
  toasts, the generator preview.
- **16px** (`--radius-lg`) — everything that covers: modals and the unlock panel.
- **999px** — pills: chips, folder tags, counts, step markers, meter cells, the switch
  track, the TOTP bar, the clipboard badge.

Outside the scale, two small exceptions are fixed by context rather than design: the
30px/44px brand tiles and item avatars use 9px and 13px (a squircle that reads as a
rounded square, not a pill), and `kbd` uses 5px with a 2px bottom border so it reads as a
physical key. Form errors, warnings and confirmations reuse `--radius-sm`; the
destructive confirm strip uses 10px.

Borders are always **1px** and always hairline, and they are functional: they define the
plane, mark focus (via the ring), or carry semantics (danger/amber mixes at 35–45%).
There is no 2px border anywhere except the inset 2px accent bar marking the active nav
item, and no clipping except the logo tile.

### Named Rules

**The Three-Radius Rule.** Before adding a number, ask which of the three jobs the
surface has — touch, group, or cover. A fourth radius means a fourth job, and there isn't
one.

**The Border-Over-Fill Rule.** A new state is introduced with a border colour plus a
washed background, not with a solid fill. Solid fills belong to the primary button and
the active surfaces, and they are rare on purpose.

## Components

Flat, bordered, unornamented: a component is a plane with a hairline and a state, and
state changes are background shifts plus a 2px focus ring.

### Buttons

- **Shape:** 6px radius, 1px hairline, inline-flex, 0.35rem gap for icon+label.
- **Default:** Elevated Plane fill, Ink text, 0.45rem × 0.75rem padding, 0.82rem/500.
  Hover shifts to Hover Plane and escalates the border to Hairline Strong.
- **Primary:** solid Signal Blue with Signal Ink text; hover lifts to Signal Blue Lifted.
  One per surface, at the decision point.
- **Danger:** Refusal Red text on a 14% red wash with a 35% red border — a filled red
  button would overstate a single click.
- **Ghost / icon:** `.icon-button` is a 30px square (22px in its `--small` variant),
  transparent until hover, muted until hover. Disabled drops to 0.4–0.5 opacity and
  `cursor: not-allowed`.
- **Block:** `.button--block` stretches to the container width and is used for the
  unlock/creation submits.

### Chips

- **Style:** pill, 1px hairline, Elevated Plane fill, 0.74rem Ink Muted. The `--quiet`
  variant shrinks to 0.68rem and Ink Faint for folder memberships on an item row.
- **Action:** `--action` switches to Signal Wash + Signal Blue text so a chip that opens
  something is visibly different from a chip that labels something. `--warn` does the
  same in Attention Amber. Removal is a `<button>` with `opacity: 0.6`, going full-opacity
  and red on hover.

### Cards / Containers

- **Corner Style:** 10px.
- **Background:** Panel Plane by default; Signal Wash for `card--secret` (the one surface
  allowed to look "highlighted", used for the vault's agent-access token).
- **Shadow Strategy:** hairline highlight only (see Elevation).
- **Border:** 1px Hairline; 45% Signal Blue for a secret card.
- **Internal Padding:** 1rem × 1.1rem on pages, 0.75rem × 0.85rem in the detail grid.

### Inputs / Fields

- **Style:** Input Well fill — darker than the panel, so a field reads as recessed —
  1px hairline, 6px radius, 0.5rem × 0.65rem padding, full width with `min-width: 0` so
  it can shrink inside a grid.
- **Focus:** 2px Signal Blue outline at 2px offset, applied globally to inputs, selects,
  textareas, buttons, links and `[tabindex]`. It is never removed, never replaced by a
  colour-only change.
- **Label:** 0.76rem/550 Ink Muted above the control, with an optional 0.74rem Ink Faint
  hint below and a 0.76rem Attention Amber warning line where validation is advisory.
- **Error:** a form-level strip in `--danger-soft` with Refusal Red text, above the
  actions. Fields are not outlined red; the sentence does the work.

### Navigation

The sidebar is a 248px Panel Plane rail holding three zones separated by hairlines:
folders (N:N, with protected system folders marked by a pill tag and a saturated icon),
tools (health, import, routing, AI access, sync, backups), and a footer with build info.
Nav items are 6px-radius rows at 0.45rem × 0.55rem in Ink Muted; hover moves to Hover
Plane and full Ink; the active item takes Active Plane **plus an inset 2px Signal Blue
bar on the left edge** — the only 2px element in the system. Counts and system tags are
pill micro-labels. Per-folder actions (`rename`, `delete`) are hidden at
`opacity: 0` and revealed on row hover or `:focus-within`, so the rail stays calm but
stays keyboard-reachable.

### Item Rows

A 10px-radius row: 30px initials avatar on a Signal Wash tile, then title (550 weight,
with a 12px amber star when favourite), then a metadata line (type icon + hostname) in
Ink Muted at 0.76rem, then any folder chips. Active rows take Active Plane + Hairline
Strong. Hover is a plain background shift — a row never lifts.

### Modals

A 540px panel (760px when `--wide`), 88vh max height, 16px radius, Hairline Strong border,
Overlay lift, on a 62% dark blurred backdrop. Structure is fixed: header (headline +
optional 0.78rem muted description + 30px close icon-button), scrolling body, optional
footer. Focus moves to the first control on open, returns to the opener on close, and
Escape closes. This is the only place the app interrupts.

### Secret Display

Secrets never render as plain text by default. Masked: `--text-muted` with 0.08em
letter-spacing; revealed: full Ink on a Signal Wash with 4px padding. TOTP codes get
`accent-strong` at 1.3rem/0.12em with a 4px countdown bar (pill, `--accent` fill, 1s
linear transition). Copy actions are icon-buttons beside the value, and a clipboard badge
confirms for a beat.

### Strength Meter

Five 4px pill cells on an Elevated Plane track, plus a text label — the score is never
conveyed by colour alone. 0–1 Refusal Red, 2 Attention Amber, 3–4 Verified Green; only
the filled cells take colour, the rest stay track-grey.

### Switch

34 × 18px pill track with a 12px thumb. Off: Elevated Plane track, Hairline Strong
border, Ink Faint thumb. On: Signal Wash track, 55% Signal Blue border, Signal Blue
Lifted thumb translated 16px. The native input is visually hidden but focus-visible still
draws the accent ring on the track. Transitions are 120ms on background and both
transforms; reduced-motion collapses them to 0.01ms globally.

### Toasts

Bottom-right, 380px max width, 10px radius, Elevated Plane on Overlay lift. Success and
error variants tint the border to 45% green/red and the text to full green/red; the
Sensitive Clipboard badge sits in the same corner as a pill. Toasts never carry a
destructive default action.

### Empty States

Centred, 0.85rem Ink Faint, generous 2rem padding, with a Signal Blue icon above and a
muted line of explanation. The detail pane's empty state is the same pattern with a
Headline. Empty states explain what to do next; they never shrug with "rien ici".

### Keycaps

`kbd` is a 5px-radius chip with a 2px bottom border, Elevated Plane, Ink Muted, 0.72em,
`white-space: nowrap`. Shortcuts are shown inline in the search field and hints, because
keyboard operability is a product requirement rather than a power-user extra.

## Do's and Don'ts

### Do:

- **Do** author every new value as a CSS custom property in `:root`, then add its light
  theme counterpart in `:root[data-theme="light"]`.
- **Do** reach for a hairline plus a tonal step before reaching for a shadow.
- **Do** pair every semantic colour with text — "refusé", "expiré", the score label.
- **Do** keep the global 2px accent focus ring on every interactive element, including
  custom switches and hover-revealed row actions.
- **Do** cap reading columns at 60ch (prose) and 900px (page), and use
  `auto-fit / minmax` for any multi-column block.
- **Do** use 6px for touch targets, 10px for groups, 16px for overlays, 999px for pills.
- **Do** write French copy as plain sentences: what failed, then what to do next.

### Don't:

- **Don't** introduce a second accent, a gradient, or a decorative illustration. The
  only gradient in the app is the 140° accent→violet logo tile.
- **Don't** hard-code a hex, `rgb()` or `rgba()` inside a component rule; consume the
  token.
- **Don't** lift a surface on hover, add a glow, or animate a transform other than the
  switch thumb.
- **Don't** borrow consumer password-manager visual language: no shield or padlock
  ornament, no "secure" badge as decoration, no oversized friendly typography, no
  vault-as-hero imagery. Security is shown through visible masking, scope and audit
  states.
- **Don't** use colour as the only carrier of a state, a score or an error.
- **Don't** widen the base font or the control padding for "breathing room"; density is
  the product. Space comes from section gaps, not from loosening controls.
- **Don't** add a fourth radius, a third shadow, or a 2px border (the active nav bar is
  the sole exception).
- **Don't** put text on a Signal Wash — it is a wash, not a fill. Solid accent fills take
  `--accent-contrast` only.
