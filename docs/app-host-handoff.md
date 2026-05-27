# App host — how it works (working handoff)

> **Status: working / temporary.** This is a *shared-understanding* document, not the
> final spec and not a build plan. It explains the **mental model** and the **why** behind
> the app system we've been designing, so a fresh reader (or a new session) can reason about
> the deeper workings before writing more code. It will be replaced by a proper `app-host.md`
> + the code once the model settles. Nothing here is locked.
>
> Read it top to bottom once; each section builds on the last. There's a glossary at the end.

---

## 1. What we're building

A **browser for a peer-to-peer world** — and the single most useful way to hold it in your
head is: **it is a small operating system for apps.**

Instead of typing a URL to visit a website, you open a **link** that subscribes you to a
**workspace** — a team space, a community, a shop, a forum — which *anyone* can host and which
can be private or public. A workspace contains **pages**, and pages run **apps**. An app is a
little program that draws its own screen and works with some data.

Two metaphors run through everything below, and both are exact, not loose:

- **It's a browser.** Workspace ≈ a site you visit by link; page ≈ a page on that site; app ≈
  the interactive content on the page. The difference that makes it *safer* than the web: apps
  have **no network** — all their I/O is shared documents that sync by themselves (more in §5).
- **It's an OS.** The shell is the kernel + window manager; each open page is a process; each
  app is a window. This is the metaphor that makes the isolation and threading make sense (§2, §6).

---

## 2. The mental model: it's an operating system

| OS concept | Our system |
|---|---|
| Kernel + window manager | the **shell** — owns the screen, input, GPU, and (later) the keys + network |
| A process | an **open page** — its own thread now, its own OS process later; owns its data |
| A window | an **app** — drawn by the page, placed on screen by the shell |
| The window manager tiles/floats windows | the shell tiles/floats/tabs **app surfaces** |
| Processes are isolated; they talk via the kernel | apps see only their page's data; they touch the network/keys only *through* the shell |
| Green threads now → real processes later | trusted apps run in-process now; untrusted apps get real OS processes later |

If a sentence below ever feels arbitrary, map it back to this table — it almost always falls out
of "treat the shell as a kernel and each page as a process."

---

## 3. Every app needs two things: a screen and some data

That's the whole job of an app, and the system is organized around giving it exactly those two
things and nothing else:

- **A screen.** The app is a script (in a language called **Rune**) that **redraws its UI every
  frame** using an immediate-mode UI toolkit (**egui**). "Immediate mode" means: there's no
  retained widget tree the app mutates — each frame the app just *says what it wants on screen
  right now* ("a heading here, a button there"), and gets back what happened ("the button was
  clicked"). It's a sequence of function calls, which is why it's easy to hand to a script safely
  (we expose a curated set of drawing functions; that set *is* the app's UI capability).

- **Some data.** The app reads and writes **documents**. These aren't files and they aren't
  fetched from a server — they're **CRDTs** (see glossary): documents that multiple people can
  edit at once and that **merge automatically without conflicts**. The app just reads/writes its
  document; getting changes to and from other people happens invisibly underneath (§5).

Everything else in this doc is about *how those two things are wired up so that apps stay fast,
isolated, and safe.*

---

## 4. The two independent structures (the key idea)

This is the insight that ties the whole design together, so it's worth slowing down on.

There are **two separate hierarchies**, and they do **not** have to line up:

**Data / security — fixed:**
```
workspace  ⊃  page  ⊃  data container (several CRDT documents)
```
A **page** is a *data boundary*. Its documents live in one place, the page's **data container**.
The apps belonging to that page read from that container.

**Presentation — free:**
```
shell  ⊃  window(s)  ⊃  layout (tabs / tiles / floating) of app surfaces
```
The shell arranges **app surfaces** on screen however the user likes — and it can mix apps from
*different pages* in the same window.

The thing that joins the two is the **app-cell**: it is *bound to its page for data* (it reads
that page's container), but its *surface is just a tile the shell can place anywhere*, next to any
other app.

```
        ┌─── window 1 (tiled) ───┐     ┌─ window 2 (floating) ─┐
shell ──│  app A   │   app B     │     │      app C            │
        └────┬──────────┬────────┘     └──────────┬────────────┘
             │ reads     │ reads                   │ reads
        page 1 data   page 2 data             page 1 data
```

App A and App C read the *same* page's data but live in *different windows*; App B reads a
*different* page's data but shares a window with App A. **The shell neither knows nor cares which
page a surface's data came from** — it only ever moves *pixels* and *input* between cells, never
data. This is exactly a desktop window manager putting windows from unrelated programs side by
side: they share screen space and the WM, nothing else.

Why this matters: it means **layout is purely cosmetic and grants zero data access**. Tiling a
private team app next to a public community app is safe by construction, because each app only
ever holds a handle to its *own* page's data.

---

## 5. The data model (the subtle part)

Apps are, fundamentally, **views over CRDT documents**. Get this section and the rest is easy.

### One place, queried — not copied

The page's CRDT documents live in **one place** (the data container). Apps **query** that one
store; they do **not** each get their own copy of the data. "Query" here means an app reads the
specific values it needs to draw — it never duplicates the whole document.

### The document is the single merge point

This is the heart of it. There is **no separate "UI state" and "network state."** Everything
funnels through the one document:

```
   peers  ⇄  (network, later)  ⇄  data container  ⇄  app's view (reads to draw)
                                        ▲   ▲
                          remote edits ─┘   └─ the user's edits
                                  (both merge into the one document)
```

- A **user edit** (clicking, typing) is just a change to the document.
- A **remote edit** (someone else, over the network — later) is also just a change to the document.
- Both **merge** into the same document; the CRDT guarantees they combine without conflict.
- The app's **view reads** the merged document to draw.

So "data from the network merges in" and "the UI's edits go out to the network" are the *same*
mechanism seen from two sides: edits become document changes, the document merges them, and they
propagate. The app never has to think about any of it — it reads and writes one document.

### It's local-first and conflict-free

- **Local-first:** your edit applies *instantly* to your local document; sending it to others
  happens in the background. You never wait on the network.
- **Conflict-free:** if you and someone else edit at the same time, the CRDT merges both into a
  state everyone converges on. The app never sees a "merge conflict" — just the merged result.

### Why apps need no network

Because the *only* way data moves is through these auto-syncing documents, and the shell/runtime
handles that movement. An app never needs to open a socket. This isn't a restriction we impose to
be safe — **the network simply isn't in the app's world.** Its entire universe is its page's
documents. (And that's *why* it's safe — see §8.)

---

## 6. Isolation: why one slow or bad app can't hurt the others

There are **two different kinds of isolation**, and conflating them is the easiest way to get
confused, so we keep them separate:

1. **Performance isolation** — one slow app shouldn't freeze the others.
2. **Security isolation** — one malicious app shouldn't be able to read others' data, the keys,
   or reach the network.

They're solved by different mechanisms at different granularities.

### Performance isolation = each app on its own thread + the compositor

Each app runs on its **own thread** and draws into its **own surface** (think: its own little
picture). Each frame, the app produces an updated picture and hands it to the shell. The shell —
on a *separate* thread — just **composites the latest picture it has from each app** onto the
screen, every refresh.

The consequence: if one app is slow, it simply stops handing over fresh pictures for a moment —
the shell keeps showing its last one and keeps every *other* app perfectly smooth. **A slow app
goes stale, it doesn't stall anything.** This is exactly how a compositor like Wayland, or a
browser's renderer, stays smooth even when one tab is busy.

### "But the apps share the data — won't a slow app block the others through the data?"

This is the real subtlety, and it's why the data store is **read as a shared, lock-free view**:
the data container hands every app a *snapshot* it can read freely, even hold across a slow frame,
**without blocking anyone** — because writes don't modify what a reader is holding; they publish a
*new* snapshot for the next read. So apps share *one* store (no per-app copy) *and* a slow reader
can never stall a writer or another reader. (There's a simpler alternative — a shared lock — but
it requires apps to read briefly and let go; the snapshot avoids that discipline. This is one of
the open choices in §11.)

### Security isolation = each page in its own process (later)

Performance isolation (threads) does **not** protect memory — a misbehaving app on a thread could,
in principle, reach into another's memory or the keys. For *untrusted* apps, the real boundary is
an **OS process per page**: a page's process holds only that page's data and **never the keys**;
the keys, the network, and the canonical data stay in the trusted core. Then even a fully
compromised app is sealed in a process with nothing valuable in it.

### The ladder

We don't build all of this at once. We start trusted and in-process, and tighten as needed:

```
now:    everything in one process, one thread per app   → performance isolation, trusted apps
next:   one thread per page/app, compositor             → smooth multi-app
later:  one OS process per page                          → security isolation, untrusted apps
```

The important property: **moving an app from a thread to a process changes nothing about the app
itself** — see §7 for why.

---

## 7. How the pieces talk

The parts communicate by **passing messages over channels**, *not* by sharing memory. (Reads of
the data store are the one shared thing, via the snapshot in §6 — everything else is messages.)

```
            OS input ──▶  SHELL  (compositor + window manager + kernel)
                           │  ▲
            input events   │  │   "here's my new picture" (a surface)
            (clicks, keys)  ▼  │   "please repaint me"
                       APP-CELL  (its own thread: runs the Rune view, draws a surface)
                           │  ▲
            "I edited X" ──┘  └── reads ── DATA CONTAINER (the page's documents; single writer)
                                                 ▲
                                  (later) remote edits from the RUNTIME (network + keys)
```

- **Shell → app:** input events (translated to that app's local coordinates), resize, "you're
  hidden, stop drawing," "close."
- **App → shell:** "here's my freshly drawn surface," "please repaint me."
- **App ↔ data container:** the app *reads* the shared snapshot; *writes* (the user's edits) go to
  the container, which is the single writer that applies them and publishes a new snapshot.
- **Runtime ↔ data container (later):** the runtime owns the network and keys; it feeds remote
  edits into the container and broadcasts the container's local edits out.

**Why message-passing instead of shared memory everywhere?** Because it's the same boundary
whether the app is a *thread* or a *process*. In-process, a "message" is a value sent down a
channel; across a process boundary it's the same message sent down a pipe. So when we later move
untrusted apps into their own processes, **the app code and the wiring don't change — only the
transport underneath does.** That's the payoff of designing this boundary now: we're writing the
"system call" interface of our little OS, and we want it to look the same regardless of where the
app runs.

---

## 8. Trust & security

The rule, stated once: **an app sees only its own page's data — no other pages, no network, no
keys.**

- **No keys.** The user's cryptographic keys live in the trusted core (the shell/runtime), never
  in an app. Apps get *decrypted, permit-scoped data* to work with, never the keys themselves.
- **No network.** Apps can't open connections. The runtime does all networking.
- **The non-obvious part — the sync fabric is itself a channel.** "No network" doesn't *by itself*
  make an app safe, because the auto-syncing documents *are* a way data leaves the machine. A
  malicious app that could reach another page's writable document could smuggle a secret out
  through sync. That's why page-scope has to be **enforced**, not just promised — which is what the
  per-page process boundary (§6) is for. Inside its own process, an app simply can't reach another
  page's documents to leak through.
- **The trade for "no network."** It removes the entire class of web attacks (phone-home,
  tracking, data exfiltration) outright — there's no channel to abuse — at the cost that apps that
  *genuinely* need outside data (a weather widget, a payment) must get it through the
  runtime/node, not directly. That's a deliberate, central choice.

The threat we design against is **malicious app code** and **malicious peers** — not a compromised
shell/runtime. The trusted core is trusted; if it's compromised, everything is. We don't try to
defend against that.

---

## 9. The plan: simple now, strong later

We deliberately build the **trusted, in-process** version first, and the contract (the message
boundary in §7, the `view`-draws-each-frame model, the page-scoped data handle) is what keeps the
*untrusted, multi-process* version an upgrade rather than a rewrite.

Rough order (concepts, not a schedule):

1. **The app-cell as a self-contained unit** — owns its UI, produces a surface the shell
   composites. (Right now an app draws straight into the shell; this is the first real change.)
2. **Several app-cells + windowing** — tiles / floating windows / tabs, each app on its own thread.
   This is where performance isolation becomes real and visible.
3. **Real data** — the page's data container holding actual CRDT (Loro) documents, with apps
   reading the shared snapshot and writing through the single writer.
4. **Networking** — the runtime brokers sync between pages and peers (and the keys/encryption).
5. **Untrusted apps** — each page in its own OS process; the same message boundary, now over IPC;
   permits enforced.

---

## 10. Where we are right now

A **working counter app**:

- written in **Rune**, drawn with **egui**, running inside the shell **right after you log in**;
- clicking the buttons actually changes the number (so state persists across frames);
- a headless test renders a frame with no window, proving the script + UI + state wiring end to end.

It is the simplest possible proof that *a script can draw a UI and hold its state*. It is **Stage
1**: one app, one thread, one screen, trusted, baked into the binary. None of the data model
(§5), isolation (§6), or multi-app windowing (§4) is built yet — that's what the plan above is.

(Concrete facts for the curious: the code is the `app_host` crate plus a few lines wired into the
shell's home screen; Rune 0.14.2, egui 0.34.2.)

---

## 11. Open questions (genuinely undecided)

- **Read view: snapshot vs. lock (§6).** A lock-free shared snapshot (best isolation, one shared
  materialization per change) or reading the live document under a brief lock (zero extra copy, but
  apps must read-and-release quickly). Leaning snapshot; may get both if the CRDT library's
  versioned views are cheap.
- **When real data (Loro) arrives** — wire it into the very first windowing work (strong
  foundation) or get windows working with the toy counter state first and add data right after.
- **How a surface crosses to the shell** — as drawing commands (simpler, recommended) or as a
  finished GPU image (needed for very heavy apps and across processes; upgrade later).
- **How the workspace permit/role model wires into the runtime** — deferred to networking.
- **Container widgets** (rows/columns that take a block of UI) — not built yet; needed for richer
  layouts inside an app.

---

## 12. Glossary

- **Workspace** — the thing you subscribe to by link; a community/team/shop. Anyone can host one.
  Membership boundary.
- **Page** — a *data boundary* inside a workspace. Holds documents; hosts one or more apps.
- **Layer / document** — one CRDT document. A page's data container holds several.
- **Data container** — the one place a page's documents live; apps query it, it's the single
  writer for edits.
- **App** — a Rune script that draws a UI (each frame) and reads/writes its page's documents.
- **App-cell** — the running instance of an app: its own thread + its own UI surface + a read
  handle to its page's data. The movable "window."
- **Surface** — the picture an app produces each frame, which the shell composites onto the screen.
- **Shell** — the main program: window manager + compositor + (later) the trusted core holding keys
  and the network. The "kernel."
- **Runtime** — the trusted services (networking, keys, sync) the shell owns; brokers data between
  pages and peers. (Often spoken of together with the shell as "the trusted core.")
- **CRDT** — a document type that many people can edit concurrently and that merges automatically,
  with no conflicts and no central server. (We use the **Loro** library.) The reason sync is
  invisible to apps.
- **Rune** — the scripting language apps are written in; pure-Rust, sandboxed by construction (an
  app can only call what we hand it), with a runaway-execution budget.
- **egui** — the immediate-mode UI toolkit apps draw with (each frame redescribes the screen).
- **Compositor** — the part of the shell that draws each app's latest surface onto the screen,
  every refresh, without ever waiting on a slow app.
- **Local-first** — your edits apply instantly and locally; syncing to others happens in the
  background; you never block on the network.
