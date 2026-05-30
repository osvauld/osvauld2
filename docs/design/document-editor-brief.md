# Design brief — The Document editor (`.doc`) in Osvauld

> Handoff prompt for a design exploration. Desktop-first, scaling to Linux phones later.
> Paste this into Claude Design (or any design collaborator). Start by reflecting the model
> back in your own words and proposing the **in-brand translation** (type scale, spacing, the
> "feel") *before* designing any one screen.

---

## 0. Your task

Design the **visual language and interaction design** for Osvauld's **block editor** — the
`.doc` app — on desktop now and Linux phones later. It is a **Notion-class block editor
translated into Osvauld's dark, square, retro-terminal aesthetic** — not a Notion reskin, and
not a light/rounded/friendly tool. **Presence (live multiplayer cursors) is part of the look
from the start.** Explore the in-brand translation and the signature interactions, pressure-test
them, then produce wireframes/flows as on-brand React/JSX mockups.

## 1. What Osvauld is (context)

Osvauld is a **local-first, end-to-end-encrypted, peer-to-peer** application platform. No
servers: your identity is a cryptographic keypair (a DID) on your devices; data syncs directly
between peers (CRDTs over QUIC) and works offline. Inside Osvauld's desktop shell ("Sthalam"),
apps render side by side over shared data.

The `.doc` editor is one of the **native built-in apps** — compiled into the shell, trusted,
fast, sharing the host's renderer. It lives inside an app **cell** that the shell frames.
**The shell owns the outer chrome — tabs, window controls, navigation, permissions. The editor
owns only the editing surface inside the cell.** Do **not** design app-window chrome, tabs, or
space/page navigation (that's a separate brief). Design the **canvas**.

## 2. What you're designing

A **block editor**:

- A document is an ordered **tree of blocks**; each block is a typed unit (paragraph, heading,
  list item, to-do, quote, code, …). Blocks can nest (lists/to-dos in v1).
- The **editing feel** matches Notion — a `/` slash menu to insert, markdown shortcuts
  (`# `, `- `, `> `…), drag-to-reorder, hover affordances, an inline-format toolbar on
  selection. But the **visuals are Osvauld**: near-black, square corners (0 radius), hairline
  borders, monospace "tags", a single purple accent, a retro-terminal calm.
- It is **collaborative**: design as if several people edit at once. Remote cursors and
  selections are **visible and first-class**. (Data sync is engineering; the *look* of presence
  is yours to design.)

**Key principle:** quiet, precise, high-contrast, **no rounded friendliness**. A serious,
beautiful long-form writing surface that happens to be dark and square.

## 3. Functionality to design for

Focus mockups on **v1**, but keep the visual system extensible to the **later** set.

**Block types** — v1: paragraph, H1/H2/H3, bulleted list, numbered list, to-do (checkbox),
quote, code block (monospace, *no* syntax highlight yet), divider. Later: toggle (collapsible),
callout, columns, table, image/file, embeds/bookmarks, math, table-of-contents, sub-page,
synced block.

**Inline formatting** — v1: bold, italic, strikethrough, inline code, link. Later: underline,
text/highlight color, inline math, @mentions, inline comments.

**Editing & interaction (the bulk of the feel)** — v1: caret movement (char/word/line/doc);
selection (shift / drag / double=word / triple=block); **cross-block selection**; copy/cut/paste
(rich + plain); undo/redo; Enter = new block; Backspace-at-start = merge or convert; the **slash
`/` menu**; **markdown shortcuts**; Tab/Shift-Tab indent for lists; **drag-handle reorder**;
per-block menu (turn-into / duplicate / delete / copy-link); placeholder text; keyboard
shortcuts. Later: whole-block selection mode, nested drag, color via menu.

**Nesting** — v1: one level of indent/outdent for lists & to-dos. Later: arbitrary nesting.

**Presence** — v1: paint the **local** caret + selection, plus a demo **remote** peer caret to
prove the treatment. Later: many real remote cursors, selection highlights, an identity stack.

## 4. The hard design questions (the crux)

Propose options with trade-offs.

1. **The in-brand translation — the central question.** Notion is light, rounded, friendly.
   Osvauld is near-black, square (0 radius), hairline-bordered, retro-terminal. What does a
   *serious, beautiful* block editor look like in this language? Show the **type scale**
   (H1–H3, body, captions), the **vertical rhythm/spacing**, line length for reading, and how
   "quiet but precise" holds up across a long document.
2. **Block affordances.** How does a block reveal its **drag handle** (⠿) and **add (+)**
   button — a left gutter on hover? How minimal can the *resting* page be (clean, writerly)
   while keeping affordances discoverable? Show **resting / hover / focused / selected** states.
3. **The slash `/` menu** — the signature insert interaction. Layout, grouping, fuzzy search,
   icons, keyboard navigation, and the dark/square treatment. (Same family as the shell's
   command palette, but block-scoped.)
4. **Inline formatting toolbar.** On text selection: a floating toolbar (bold/italic/strike/
   code/link), keyboard-only, or both? Its on-brand treatment and where it sits relative to the
   selection.
5. **Presence / remote cursors — core, not garnish.** Each peer = a colored **caret** + a
   **name label** (monospace tag, in the spirit of the shell's `↩ UNLOCK · ESC BACK` tags) + a
   **selection highlight** in the peer's color. Design: where labels sit; avoiding clutter with
   several peers; an **identity stack** (who's here) at the top of the doc; active vs idle;
   rendering a person humanely (name + identicon + short DID, as the shell already does).
6. **Drag-to-reorder.** The **drop-indicator line**, the dragged ghost, and where a block can
   land (including into/out of a nesting level). Square, hairline, accent.
7. **Nesting & indentation.** Indent guides? Disclosure arrows for lists? How does depth read
   cleanly against a near-black background?
8. **Per-type treatment.** To-do (**square** checkbox; done = muted + strikethrough), quote
   (accent rule), code block (monospace, a language **tag**, monospace even without highlight),
   divider, heading hierarchy. Show them together in one realistic document.
9. **Title & empty states.** The doc **title** at the top (Inter, large — **not** VT323, which
   is wordmark-only); first-run empty doc ("Type `/` for commands"); empty-block placeholder.
10. **Focus & reading.** What marks the focused block and the caret line without making
    long-form reading busy? Keep it calm.
11. **Desktop → phone.** Block editing on a 6″ screen: where do the hover affordances go
    (long-press? a persistent handle?); the slash menu (full-screen sheet?); the inline toolbar
    (docked above the keyboard?); drag-reorder by touch. **Graceful degradation, same product.**

## 5. Constraints & principles

- **Keyboard-first.** Everything reachable without a mouse; deliver a keyboard map.
- **Rendered natively in egui + cosmic-text — not a web view.** Treat the React/JSX mockups as
  an **aesthetic + interaction north-star**, and **stay within effects that map to a GPU
  painter**: solid fills, hairline (1px white-alpha) borders, square corners, flat color, simple
  opacity fades. **Avoid** backdrop blur, layered drop shadows, heavy gradients, and complex CSS
  transitions/filters — they won't translate and will mislead the build.
- **Performance-aware.** Long docs are virtualized (only visible blocks are laid out/painted).
  Avoid per-block heavy decoration that implies expensive paint. Calm is also cheap.
- **The editor is inside a cell.** Don't draw window chrome, tabs, or navigation — the shell
  owns those. Design only the editing surface and its in-canvas affordances (menus, toolbars,
  presence).
- **Don't over-show the plumbing.** DIDs, permits, CRDTs are the engine, not the dashboard.
  Presence shows *people*, not protocol.

## 6. Visual language (stay on-brand)

Match Sthalam's existing identity:

- **Dark, near-black** canvas `#0A0B10`; layered grays `#0D0E13 → #14151C → #1C1D27 → #262735`.
- Foreground `#F5F5F7`; muted `#7F8192`; faint `#4D4E5C`.
- **One accent** — soft purple `#8A86E5` (hover `#A09DEE`, press `#6E6AD0`).
- **Square corners everywhere (0 radius)** — a signature; hairline white-alpha borders.
- **Type:** Inter (UI + body), JetBrains Mono (labels/metadata/tags/code), VT323 pixel face
  (wordmark only — *not* for doc titles). Mono "corner tags" like `02 / login · unlock` and
  shortcut hints like `↩ UNLOCK · ESC BACK` are part of the look.
- Semantic: ok `#7EE787`, warn `#F5C06A`, err `#F47068`. Use distinct, legible **peer colors**
  for presence drawn from the same high-contrast-on-dark family.
- Feel: quiet, precise, **retro-terminal / pixel**, high-contrast, no rounded friendliness.

## 7. Deliverables

1. **Reflect the model back**, then propose the **in-brand translation** (type scale, spacing,
   the "feel") as 1–2 directions, each a one-line thesis with trade-offs.
2. For the chosen direction, **wireframes/flows** for: a full document with mixed blocks; block
   **hover / focus / selected** states; the **slash menu**; the **inline-format toolbar**;
   to-do / quote / code / heading treatments; **presence** (2–3 remote cursors + selections +
   identity stack); **drag-to-reorder**; **nesting**; **empty / first-run**; the **phone**
   adaptation.
3. A **keyboard map** for the core actions.
4. On-brand **React/JSX mockups** (Tailwind fine) using the tokens above — dark, square corners,
   **painter-safe effects only**.
5. A short list of **open decisions** handed back to engineering.

Reflect the model back first, propose the in-brand translation, then design.
