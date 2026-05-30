# Design brief — Navigating Spaces, Pages & Apps in Osvauld

> Handoff prompt for a design exploration. Desktop-first, scaling to Linux phones later.
> Paste this into Claude Design (or any design collaborator). Start by reflecting the model
> back in your own words and proposing 2–3 navigation directions *before* designing any one.

---

## 0. Your task

Design the **information architecture and navigation UX** for Osvauld — specifically how a
person moves through and makes sense of **Spaces → Pages → Apps**, on desktop now and on
Linux phones later. Explore multiple navigation directions, pressure-test them against the
real examples in §3, and produce wireframes/flows as on-brand React/JSX mockups. Don't jump
to one answer — compare directions first.

## 1. What Osvauld is (context)

Osvauld is a **local-first, end-to-end-encrypted, peer-to-peer** application platform. No
servers: your identity is a cryptographic keypair (a DID) that lives on your devices; data
syncs directly between peers (CRDTs over QUIC) and works offline. Inside Osvauld's desktop
shell ("Sthalam"), small **sandboxed third-party apps** render side by side and operate on
shared data. Think *a local-first, P2P alternative to a workspace suite* — except the apps
are sandboxed and untrusted, and **you own the data and decide who can see it.**

## 2. The model you're designing navigation for

Three nested concepts:

- **Space** — the outer boundary and the unit of **sharing/access**. A Space groups related
  work (like a Miro workspace, a Slack workspace, or a Notion teamspace). **You invite people
  to a Space**; that membership is the coarse access boundary — not in the Space, see nothing
  inside it.
- **Page** — lives inside a Space. A Page holds **data** (typed collaborative
  documents/records) and hosts **multiple apps**. Apps on a Page read/write that Page's data
  and can add to it. *(Original Osvauld allowed only one app per Page; the new model is **many
  apps per Page** — this is the big shift, and it's why the shell now has a window manager.)*
- **App** — a sandboxed mini-application placed on a Page (a product table, a chat, a
  whiteboard, a chart…). It sees only its Page's data, scoped further by the viewer's role.

**Key principle: data belongs to the Page, not to an app.** A Page's data is a shared
substrate; the apps on it are interchangeable **lenses** that read/write that same data. So a
Page typically hosts *several* apps over one dataset — e.g. an **admin app** and a **separate
app for everyone else** — rather than one app that owns its own data.

**Two layers of access** the UX must make legible:
1. **Space membership** — who's allowed in at all (granted P2P to a person's DID / contact).
2. **Per-app role-gating** — your role decides **which apps you can open** and **what data each
   lets you touch**. The admin app and the customer app sit on the same Page over the same
   `products`/`orders` data; an owner can open both, a customer only the customer app.

## 3. Ground every idea in these real apps (they exist as demos)

- **My Shop (e-commerce, role-gated) — the key access example.** One Space shared between a
  shop owner and customers. A Page carries `products` (shared) and `orders` (per-customer,
  private) data. It hosts a **Shop Owner** app (owner/admin only: manage products, see all
  orders) and a **Shop Customer** app (customers: browse, order, see only *their* orders).
  Same Space, same Page data — **owner and customer see different apps and different slices.**
  Restriction is declared per app as an allow-list of roles.
- **Group Chat** — channels, threads, reactions, DMs (many sub-streams in one app).
- **Canvas / Whiteboard** — collaborative shapes, connectors, **live cursors (presence)**.
- **Photo Gallery** — shared albums, large blobs syncing P2P.
- **Snake / Tank (multiplayer games)** — real-time, synced leaderboards.
- **Block/Text Editor, Math Sim, Docs** — documents and continuous data.

A single Page might therefore hold a chat + a whiteboard + a data table at once — three
separate sandboxed apps sharing the Page's data.

## 4. The hard questions the design must answer

Propose options with trade-offs; these are the crux.

1. **Hierarchy & orientation.** How do you navigate Space → Page → App and always know where
   you are? Sidebar tree? Breadcrumb? Spatial/zoom? The prior design was a *folder → folder →
   app* tree — explore whether Pages need grouping/nesting (folders) or whether three flat
   levels suffice. **Where does the "Miro canvas" metaphor belong** — at the Space level (a
   board of Pages), the Page level (a canvas of apps), or both?
2. **The Page as a multi-app surface.** A Page hosts several apps at once. On desktop we're
   building a window manager — apps as **tabs / tiles / floating windows**, draggable between
   all three. How does that in-page layout coexist with switching Pages and Spaces? Is a Page
   a free spatial canvas (pan/zoom) or a managed dock? How do you **add an app** to a Page and
   discover which apps can be added?
3. **Role views via multiple apps.** A Page is shared data + a set of apps; your role decides
   which apps you can open and what each lets you touch (My Shop: an **admin app** for owners
   and a **separate app for customers**, both over the same `products`/`orders` data). On a
   Page, how do you show apps you *can't* open — hidden, or a locked badge? How does an owner
   **compose** a Page: add apps and assign which roles get which?
4. **Command palette (Raycast-style).** A keyboard launcher is core. Define what it does:
   jump to any Space / Page / App; run app actions; create a Page/App; invite people; **search
   across data in apps you can see**? Is it the primary navigator (especially on phone), or a
   power accelerator on top of visual nav?
5. **People, sharing & trust (P2P).** No central directory — "people" are contacts/DIDs.
   Design: inviting someone to a Space, assigning a role, seeing who's in a Space, revoking.
   Show **presence and sync state** (peer online/offline, data stale/syncing) since it's
   offline-first. Render a DID humanely (name + identicon + short DID, as the shell already
   does).
6. **Desktop → Linux phone.** The same model must work on a phone. A tiled multi-app Page
   can't tile on a 6″ screen — does it become a stack / switcher / tabs? Do Space & Page nav
   become a drawer or bottom-nav? Does the command palette become the primary navigator?
   Design **graceful degradation, not a separate product.**
7. **Empty & onboarding states.** First run: create your first Space, add a Page, drop in an
   app, invite a person — legible to non-technical users despite the crypto/P2P underpinnings.

## 5. Constraints & principles

- **Keyboard-first.** Everything reachable without a mouse; show the shortcut model.
- **Local-first & private by design.** No central server; surface ownership, encryption, and
  sync honestly — without nagging.
- **Sandboxed third-party apps.** Apps render into cells the shell controls; the **shell owns
  chrome, navigation, and permissions** — apps don't.
- **Don't over-show the plumbing.** DIDs, permits, CRDTs are the engine, not the dashboard.
- **Naming:** in code, `Workspace` already names the per-Page app dock (the window manager).
  Pick a clean user-facing term for the top-level boundary (Space / Workspace / …) that won't
  collide.

## 6. Visual language (stay on-brand)

Match Sthalam's existing identity:

- **Dark, near-black** canvas `#0A0B10`; layered grays `#0D0E13 → #14151C → #1C1D27 → #262735`.
- Foreground `#F5F5F7`; muted `#7F8192`; faint `#4D4E5C`.
- **One accent** — soft purple `#8A86E5` (hover `#A09DEE`, press `#6E6AD0`).
- **Square corners everywhere (0 radius)** — a signature; hairline white-alpha borders.
- **Type:** Inter (UI), JetBrains Mono (labels/metadata), VT323 pixel face (wordmark only).
  Mono "corner tags" like `02 / login · unlock` and shortcut hints like `↩ UNLOCK · ESC BACK`
  are part of the look.
- Semantic: ok `#7EE787`, warn `#F5C06A`, err `#F47068`.
- Feel: quiet, precise, **retro-terminal / pixel**, high-contrast, no rounded friendliness.

## 7. Deliverables

1. **2–3 distinct navigation directions** (e.g. "IDE: sidebar + dock", "spatial Miro-canvas",
   "command-palette-first"), each with a one-line thesis and trade-offs.
2. For the strongest direction, **wireframes/flows** for: switching Spaces; navigating Pages;
   a multi-app Page (tabs/tiles/floating); adding an app; the **My Shop owner-vs-customer**
   role views; the command palette; inviting + role-assigning a person; the **phone**
   adaptation.
3. A **keyboard map** for the core actions.
4. On-brand **React/JSX mockups** (Tailwind fine) using the tokens above — dark, square
   corners.
5. A short list of **open decisions** handed back to engineering.

Reflect the model back first, lay out the 2–3 directions, then design.
