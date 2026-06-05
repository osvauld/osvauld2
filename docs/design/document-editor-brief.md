# Design brief — The `.doc` editor in Osvauld

> Handoff prompt for a design exploration. Desktop-first, scaling to Linux phones later.
> Paste this into Claude Design (or any design collaborator). **Start by reflecting the model
> back in your own words and proposing the in-brand translation** (type scale, spacing, the
> "feel") *before* designing any one screen — then design.

---

## 0. Your task

Design the **interaction and visual language** for Osvauld's `.doc` **block editor** — a
**Notion-class editor translated into Osvauld's dark, square, retro-terminal aesthetic** (not a
Notion reskin, not a light/rounded/friendly tool) — **as it lives inside a tiling window
manager**, where the same document might be a full-width tab, a narrow tile beside two other
apps, or a small floating window.

It is a real editing **engine** (ProseMirror-class: a block **schema**, transactions/undo,
**decorations** — view-only overlays like drop indicators and presence — custom **node views**
per block type, and markdown **input rules**). So design, from the start, for **full
drag-and-drop**, **comments**, **live multiplayer presence**, and **math (LaTeX)** — all on a
**dark, square, retro-terminal, beautiful long-form writing surface**. Explore the in-brand
translation and the signature interactions, pressure-test them **across cell sizes**, then
produce wireframes/flows as on-brand React/JSX mockups.

## 1. What Osvauld is (context)

Osvauld is a **local-first, end-to-end-encrypted, peer-to-peer** application platform. No
servers: your identity is a cryptographic keypair (a DID) that lives on your devices; data syncs
directly between peers (CRDTs over QUIC) and works offline. It's shaping into a **collaborative
OS**: **spaces** of **typed files** (`.doc`, `.chat`, `.table`, `.canvas`, `.board`, `.drive`,
`.form`), each opened in its own **core app**, shared with the people on the space.

The `.doc` editor is one of those **native built-in apps** — compiled into the shell ("Sthalam"),
trusted, fast, painting with the host's egui renderer. Under the hood it's a **ProseMirror-class
engine over a CRDT**: a Loro tree of blocks, each block its own rich-text container with marks;
comments and cursors anchor to stable positions that survive concurrent edits. You don't design
the CRDT — but it's *why* presence, comments, and drag-reorder stay robust under simultaneous
editing, and why everything is attributed to **people (DIDs)**, not accounts.

**Key principle:** quiet, precise, high-contrast, **no rounded friendliness**. A serious,
beautiful long-form writing surface that happens to be dark and square.

## 2. The tiling / app-cell context — **read this first; it shapes everything**

The shell is a **window manager** (built on egui_dock). Every app — including this editor —
renders inside a **cell** that the user can arrange three ways and **drag freely between**:

- **Tab** — one of several cells in a tab group (only the active tab's body shows).
- **Tile** — a split pane sharing the screen with other apps (a `.doc` beside a `.chat` beside a
  `.table`). **Tiles can be narrow.**
- **Floating window** — a small movable window over the layout.

What this means for you:

- **The shell owns the chrome; you own only the body.** The tab strip (with the cell's title),
  split handles, the floating-window frame, the drag/drop overlays *between cells*, window
  controls, space/page navigation, and the **"export to PDF"** affordance all live in shell
  chrome — **do not design them.** You design the **editing canvas inside the cell** and its
  in-canvas affordances (menus, toolbars, comments, presence, block drag).
- **The cell is any width, and width changes at runtime.** The same doc must read well from a
  wide ~900px tab down to a ~320px tile — **"narrow tile" is a desktop reality, not just a
  phone.** The reading measure, slash menu, inline toolbar, comments, drag affordances, and math
  must all **degrade gracefully as the cell narrows**, and re-expand when it widens.
- **Everything you draw is clipped to the cell.** Pop-ups, the slash menu, the inline toolbar,
  comment threads, the drag ghost, and auto-scroll-while-dragging **cannot overflow into
  neighbouring tiles** — they open and stay *within* the cell's bounds (flip/clamp to fit).
- **Only the focused cell has the keyboard.** Design a clear **focused vs unfocused** state:
  focused = live caret, active affordances; unfocused = the document stays fully readable, the
  local caret hides, **but remote presence and comment markers stay visible**. Clicking focuses.
- **The cell body fills exactly and scrolls itself.** No shell scrollbar wraps the cell — the
  editor provides its own scrolling and any sticky elements (toolbar, identity stack).
- **Several apps share a page.** A `.doc` is often on screen next to other live, collaborative
  apps. Keep it quiet; it shouldn't shout next to a chat.

## 3. What you're designing

A **block editor** that is also a real **engine**:

- A document is an ordered **tree of blocks**; each block a typed unit (paragraph, heading, list
  item, to-do, quote, code, **equation**, divider…). Blocks nest (lists/to-dos in v1).
- The **editing feel matches Notion** — a `/` slash menu to insert, markdown shortcuts (`# `,
  `- `, `> `, `$$`…), drag-to-reorder, hover affordances, an inline-format toolbar on selection.
  But the **visuals are Osvauld**: near-black, square corners (0 radius), hairline borders,
  monospace "tags", a single purple accent, a retro-terminal calm.
- **An engine, not a toy.** Custom **node views** per type (code with a language tag, an
  **equation block**, later image/embed/table) and **decorations** (drop indicators, presence
  carets, comment highlights) are part of the visual system — design them as a coherent family.
- **It is collaborative.** Design as if several people edit at once. Remote cursors and
  selections are **visible and first-class**; **comments** (§5) and **math** (§7) are full
  surfaces; **drag-and-drop** (§6) is the signature block manipulation.

## 4. Functionality to design for

Focus mockups on **v1**, but keep the visual system extensible to the **later** set.

**Block types** — v1: paragraph, H1/H2/H3, bulleted list, numbered list, to-do (checkbox),
quote, code block (monospace, *no* syntax highlight yet), **equation (display math)**, divider.
Later: toggle (collapsible), callout, columns, table, image/file, embeds/bookmarks,
table-of-contents, sub-page, synced block.

**Inline formatting** — v1: bold, italic, strikethrough, inline code, link, **inline math**.
Later: underline, text/highlight color, @mentions.

**Editing & interaction (the bulk of the feel)** — v1: caret movement (char/word/line/doc);
selection (shift / drag / double=word / triple=block); **cross-block selection**; copy/cut/paste
(rich + plain); undo/redo; Enter = new block; Backspace-at-start = merge or convert; the **slash
`/` menu**; **markdown shortcuts**; Tab/Shift-Tab indent for lists; **drag-handle reorder** (§6);
per-block menu (turn-into / duplicate / delete / copy-link); placeholder text; keyboard
shortcuts. Later: whole-block selection mode, nested drag, color via menu.

**Nesting** — v1: one level of indent/outdent for lists & to-dos. Later: arbitrary nesting.

**Per-type treatment** — to-do (**square** checkbox; done = muted + strikethrough), quote
(accent rule), code block (monospace, a language **tag**, monospace even without highlight),
**equation block** (centered, `TEX` tag in the code-tag family), divider, heading hierarchy.
Show them together in one realistic document.

**Presence** — v1: paint the **local** caret + selection, plus a demo **remote** peer caret to
prove the treatment. Later: many real remote cursors, selection highlights, an identity stack.

**The dedicated surfaces** — **comments** (§5), **drag-and-drop** (§6), **math** (§7), and the
**focused / unfocused cell** states (§2).

## 5. Comments — a first-class collaborative surface

Comments are collaborative, attributed to DIDs, and **anchored to the text** (they ride along as
the text is edited concurrently, and gracefully become "orphaned/resolved" if their anchor is
deleted). Design:

1. **Creating** — from a text selection (inline-toolbar comment action and/or a shortcut).
2. **The anchor** — a subtle highlight/underline in a **comment colour** that must coexist with
   the **selection** (accent) and **presence** (peer colours) without collision; overlapping
   comments; hover/active states.
3. **The thread** — author (name + identicon + short DID), timestamp, replies, **resolve /
   reopen**, unread affordance. Quiet, square, hairline — not chat-bubbly.
4. **Where the thread lives — the crux, and it's a tiling problem.** A right-margin rail
   (Google-Docs style) is lovely **wide** but **impossible in a narrow tile**. Propose a
   treatment that **adapts with cell width**: margin rail (wide) → anchored popover (medium) →
   bottom sheet / list (narrow) — *same product, graceful degradation*, **clipped inside the
   cell**.
5. **Comments + presence together** — who's viewing/typing; avoiding clutter with several
   anchors and peer cursors at once.
6. **Resolved state** — how resolved comments recede without losing history.

## 6. Drag-and-drop — ProseMirror-class block manipulation

The signature structural interaction. Design the full surface, **all within the cell**:

1. **The drag handle** (⠿) — how it reveals (left gutter on hover) and reads at rest vs ready.
2. **The dragged ghost** — what travels with the cursor (a faithful, dimmed block preview;
   square, hairline).
3. **Drop indicators — two distinct kinds.** A **between-blocks** line (lands as a sibling) *vs*
   an **into-a-container** highlight (nests inside a list/toggle/quote). Make "becomes a child"
   visually different from "lands after." Show landing **into and out of** a nesting level.
4. **Multi-block drag** — dragging a selection of several blocks at once (count badge on the
   ghost); keeping their relative structure on drop.
5. **Auto-scroll** — dragging near the **cell's** top/bottom edge scrolls the doc; the ghost and
   indicators stay clipped to the cell.
6. **External drops** — dropping a file/image **into** the doc (from the OS or another app)
   creates an image/file block; show the drop target and the "release here" affordance.
7. **Open question — cross-app drag.** This is an *OS* with tiled apps. Dragging a block out of a
   `.doc` into a `.chat`/`.table` is tantalizing but a much larger, cross-cell, shell-owned
   feature. **Treat within-cell DnD (1–6) as the design target now**; sketch cross-app drag only
   as a forward-looking note, flagged as shell-level and later.

## 7. Math / LaTeX

Math is **typeset natively and painter-safe** (rendered as proper math glyphs/paths, never an
image of a webpage). Two forms:

1. **Inline math** — an inline atom inside a line of text (e.g. `$E = mc^2$`), aligned to the
   text baseline and scaling with the surrounding type. How it reads at rest; how it looks when
   the caret is beside/inside it (it edits as an **atom**, not character-by-character).
2. **Block / display equation** — a centered equation block (its own node), via `/equation` or
   the `$$` markdown shortcut.
3. **Editing math** — the source-vs-rendered affordance: a focused equation shows an editable
   **LaTeX source** field (JetBrains Mono) with a **live rendered preview**; unfocused, it shows
   only the rendered math. Design this for both inline (a small popover) and block (inline source
   area). Notion's equation editor is the reference, re-skinned to Osvauld.
4. **Error state** — invalid LaTeX: a calm, legible error treatment (not a red scream) that still
   lets you keep typing.
5. **On-brand treatment** — rendered math in the document's foreground colour and type rhythm;
   source in mono; square, hairline framing for the block; a `TEX` tag in the same family as the
   code block's language tag.

## 8. The hard design questions (the crux)

Propose options with trade-offs. These are the crux.

1. **The in-brand translation — the central question.** Notion is light, rounded, friendly.
   Osvauld is near-black, square (0 radius), hairline-bordered, retro-terminal. What does a
   *serious, beautiful* block editor look like in this language? Show the **type scale** (H1–H3,
   body, captions), the **vertical rhythm/spacing**, line length for reading, and how "quiet but
   precise" holds up across a long document.
2. **Cell-width degradation — the central *new* question.** Show the **same document and the same
   interactions at three widths**: a wide tab (~900px), a medium tile (~560px), a narrow tile
   (~340px). What happens to the reading measure, the left gutter / drag-handle / `+`, the slash
   menu, the inline toolbar, the identity stack, **comments**, and **math** at each?
3. **Block affordances.** How does a block reveal its **drag handle** (⠿) and **add (+)** button —
   a left gutter on hover? How minimal can the *resting* page be (clean, writerly) while keeping
   affordances discoverable? Show **resting / hover / focused / selected** states.
4. **The slash `/` menu** — the signature insert interaction. Layout, grouping, fuzzy search,
   icons, keyboard navigation, the dark/square treatment, and the **narrow-cell** form. (Same
   family as the shell's command palette, but block-scoped, and **opening within the cell**.)
5. **Inline formatting toolbar.** On text selection: a floating toolbar (bold/italic/strike/code/
   link **+ comment + inline-math**), keyboard-reachable. Its on-brand treatment, where it sits
   relative to the selection, and how it **clamps inside the cell** near edges / when narrow.
6. **Drop-indicator grammar (§6.3).** A clear, square, hairline visual language distinguishing
   "land as sibling" from "nest as child," in and out of levels. The dragged ghost and where a
   block can land.
7. **Math as an atom.** How inline math sits in a text line and how the caret/selection treat it
   as a single unit; how the source/preview editor feels fast and keyboard-first.
8. **Comments at every width (§5.4)** — the rail → popover → sheet progression.
9. **Presence / remote cursors — core, not garnish.** Each peer = a colored **caret** + a **name
   label** (monospace tag, in the spirit of `↩ UNLOCK · ESC BACK`) + a **selection highlight** in
   the peer's color. Where labels sit; avoiding clutter with several peers; an **identity stack**
   (who's here) that fits even a narrow cell; active vs idle; rendering a person humanely (name +
   identicon + short DID).
10. **Nesting & indentation.** Indent guides? Disclosure arrows for lists? How does depth read
    cleanly against a near-black background?
11. **Per-type treatment.** To-do, quote, code (with language tag), **equation**, divider,
    heading hierarchy — shown together in one realistic document.
12. **Title & empty states.** The doc **title** at the top (Inter, large — **not** VT323, which
    is wordmark-only); first-run empty doc ("Type `/` for commands"); empty-block placeholder.
13. **Focus & reading.** What marks the focused block and the caret line without making long-form
    reading busy? And the **focused vs unfocused cell**: an unfocused `.doc` beside a focused app
    — caret gone, but presence and comment markers alive, fully readable, not greyed into
    uselessness; returning focus on click without a jump.
14. **In-cell, never-overflow overlays.** Every pop-up (slash menu, toolbar, comment popover,
    per-block menu, link/math editor) and the **drag ghost** open and stay inside the cell. Show
    the flip/clamp near edges and in a narrow cell.
15. **Calm in a multi-app view.** Caret, presence, new-comment, and drag affordances shouldn't be
    noisy when the `.doc` is one of several live tiled cells.
16. **Desktop → phone.** Block editing on a 6″ screen: where hover affordances go (long-press? a
    persistent handle?); the slash menu (full-screen sheet?); the inline toolbar (docked above the
    keyboard?); drag-reorder by touch; the math source editor; comments as a sheet. **Graceful
    degradation, same product** — and often the **same answer as the narrow tile**.

## 9. Constraints & principles

- **Keyboard-first.** Everything reachable without a mouse; deliver a keyboard map (incl. comment
  add/resolve/next-prev; insert/edit equation; **move-block-up/down** as a keyboard alternative to
  drag).
- **Rendered natively in egui (math typeset natively; cosmic-text later) — not a web view.** Treat
  the React/JSX mockups as an **aesthetic + interaction north-star**, and **stay within effects
  that map to a GPU painter**: solid fills, hairline (1px white-alpha) borders, square corners,
  flat color, simple opacity fades. **Avoid** backdrop blur, layered drop shadows, heavy
  gradients, and complex CSS transitions/filters — they won't translate and will mislead the
  build. Math renders as glyphs/paths, so it inherits these same rules.
- **Cell-bounded.** Overlays, ghosts, and auto-scroll clip to the cell; nothing escapes into
  neighbouring tiles.
- **Performance-aware.** Long docs are virtualized (only visible blocks are laid out/painted).
  Avoid per-block heavy decoration that implies expensive paint. Calm is also cheap.
- **The editor is inside a cell.** Don't draw window chrome, tabs, navigation, the export button,
  or cross-cell drag overlays — the shell owns those. Design only the editing surface and its
  in-canvas affordances.
- **Don't over-show the plumbing.** DIDs, permits, CRDTs are the engine, not the dashboard.
  Presence and comments show *people*, not protocol.

## 10. Visual language (stay on-brand)

Match Sthalam's existing identity:

- **Dark, near-black** canvas `#0A0B10`; layered grays `#0D0E13 → #14151C → #1C1D27 → #262735`.
- Foreground `#F5F5F7`; muted `#7F8192`; faint `#4D4E5C`.
- **One accent** — soft purple `#8A86E5` (hover `#A09DEE`, press `#6E6AD0`).
- **Square corners everywhere (0 radius)** — a signature; hairline white-alpha borders.
- **Type:** Inter (UI + body **+ doc titles**), JetBrains Mono (labels/metadata/tags/code **+
  LaTeX source**), VT323 pixel face (wordmark only — *not* for doc titles). Mono "corner tags"
  like `02 / login · unlock` and shortcut hints like `↩ UNLOCK · ESC BACK` are part of the look.
  Rendered math uses a math face but reads in the doc's foreground colour and rhythm.
- Semantic: ok `#7EE787`, warn `#F5C06A`, err `#F47068`. Use distinct, legible **peer colours**
  for presence; a **comment colour** and **drop-indicator accent** that coexist with the selection
  accent and peer colours without collision.
- Feel: quiet, precise, **retro-terminal / pixel**, high-contrast, no rounded friendliness.

## 11. Deliverables

1. **Reflect the model back**, then propose the **in-brand translation** (type scale, spacing,
   the "feel") as 1–2 directions, each a one-line thesis with trade-offs.
2. For the chosen direction, **wireframes/flows** for: a full document with mixed blocks (incl.
   **code** and an **equation**); block **hover / focus / selected** states; the **slash menu**;
   the **inline-format toolbar**; **drag-and-drop** (handle, ghost, both drop-indicator kinds,
   multi-block, into/out of nesting, external drop); **math** (inline atom, block equation,
   source+preview editor, error); **comments** (creating, anchor, thread, resolved, at three
   widths); **presence** (2–3 remote cursors + selections + identity stack); **nesting**;
   **empty / first-run**; **focused vs unfocused cell**; the **same doc at wide / medium / narrow
   widths**; and the **phone** adaptation.
3. A **keyboard map** for the core actions (incl. comments, math, move-block).
4. On-brand **React/JSX mockups** (Tailwind fine) using the tokens above — dark, square corners,
   **painter-safe effects only**, **all overlays/ghosts clipped within the cell frame**.
5. A short list of **open decisions** handed back to engineering.

Reflect the model back first, propose the in-brand translation, then design — **and show it in a
cell, at more than one width.**
