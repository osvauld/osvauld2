# Blender import — binary item files, GLB scene data, textured meshes

Status: **plan, 2026-11-05; nothing built.** Continues the uncommitted `moon-rover` worktree
(gravity, fixed mesh colliders, body properties, wheel components — see
[moon rover](moon-rover.md), [composable physics](physics-components.md)). Lua-generated terrain
and rover geometry is impractical for real content; models are authored in Blender, exported as
GLB, and Lua composes and animates them over Rapier.

## Boundary (user decision, 2026-11-05)

**No model-specific Rust.** Rust provides generic pieces only:

- binary item files, stored sealed and read by Rust — Lua never handles raw bytes;
- a GLB reader that returns plain data: nodes (name, parent, TRS, Blender custom properties
  from glTF `extras`), `gfx.mesh` handles, materials and textures;
- textured materials for any mesh, imported or Lua-built;
- convex-hull colliders from any mesh (moving props), next to the existing fixed `mesh` body.

Lua decides what nodes *mean* — which are colliders, which are wheels, what moves. That rule
set is a shared Lua module (`demo_apps/moon_rover/glb_scene.lua` first), never Rust. No Rust
names a vehicle, terrain, wheel-in-a-model or naming convention.

Out of scope for this plan: skinned meshes and Blender-authored animation clips. Lua animates
by setting node/object transforms; clips-as-data and GPU skinning are a later plan.

## 1. End-to-end acceptance (write first, watch fail)

All through the real bridge, shell, Lua VM and vault, offscreen.

- **E1 `smoke_binary_files.py`** — `write_binary_file(item, "models/a.glb", bytes)` stores a
  file; `list_files` shows it with size and hash; reopening the shell (same data dir) still has
  it; the vault directory contains no plaintext of it. A bad path, an oversize file and a write
  to a locked vault refuse and leave the item unchanged. Text `WriteFile`/`edit_file` on a
  binary path refuses. A re-upload of identical bytes stores no second copy.
- **E2 `smoke_glb_scene.py`** — an app loads a fixture GLB with `gfx.load_glb("models/x.glb")`
  outside `view`, builds a `gfx.scene3d` from the returned nodes in Lua, and renders.
  `DumpTree` shows node names, parents, extras and mesh summaries that match the fixture;
  pixels show the textured face's texture colour (not the flat material colour). A truncated
  GLB, an external-URI buffer, an oversize image and a missing file each give a Lua error with
  the reason; the previous source stays live.
- **E3 `smoke_glb_rover.py`** — the moon rover app loads `terrain.glb` and `rover.glb`; its
  Lua module turns `*-col` nodes into fixed `mesh` bodies, `*-hull` into dynamic hull bodies,
  and wheel nodes into the existing wheel components. Reuse `smoke_rover_drive.py`'s driving
  assertions: keyboard throttle moves the rover over imported terrain, it collides with an
  imported rock, a hull prop gets pushed. Rendered wheel nodes spin with their wheel state
  (Lua sets transforms; no Rust animation).
- **E4 limits** — a 200k-vertex terrain GLB loads (32-bit indices), renders and collides; the
  documented per-mesh/scene/VM limits reject one past each bound before any native change.
- Existing `smoke_mesh_math`, `smoke_rover_*`, `smoke_physics_components`, mission smokes and
  `cargo test --workspace` stay green at every step.

Fixtures: `scripts/fixtures/make_glb.py` (stdlib only) writes small deterministic GLBs —
textured quad with extras, two-node hierarchy, rock, terrain grid. Checked in with their
outputs. A real Blender export from the user is added as an extra fixture when available.

## 2. Steps

**Step 1 — binary item files (vault, shell, bridge).**
Bytes are stored content-addressed as sealed vault entries
`ws/<ws>/item/<id>/file/<sha256>`; the item's source doc records only `path → {sha256, size}`,
so re-exporting a model never grows CRDT history with blob bytes. New bridge requests
`WriteBinaryFile {item_id, path, base64}` and `ListFiles`; Python client methods to match.
Limits: 64 MiB per file, 256 MiB per item (revisit with the first real terrain). Orphaned
blobs are swept when no path references them. E1 passes.
*Expert: secure-core, shell.*

**Step 2 — host file reads.** `app_host` gets a read-only resolver from item path to bytes,
used only by Rust loaders. Reload stages the resolver with the staged source, so a VM never sees
another revision's files. No Lua byte API.
*Expert: app-host.*

**Step 3 — 32-bit indices and limits.** `MeshData` indices become `u32`; raise per-mesh and
per-scene limits to what E4 needs; collider triangle budget follows. Existing meshes unchanged.
*Expert: runtime.*

**Step 4 — `gfx.load_glb`.** `gltf` crate, GLB only, embedded buffers only, no extensions
beyond a stated allowlist. Returns `{ nodes = {{name, parent, translation, rotation, scale,
mesh, extras}}, meshes = {...}, materials = {...} }`. Primitives become `gfx.mesh` handles;
missing normals are computed; non-triangle modes reject. Extras are bounded JSON → Lua tables.
Parsing validates fully before any resource is registered (counts toward the VM's existing
resource budgets). E2 minus textures passes.
*Expert: app-host, architect.*

**Step 5 — textured materials.** Vertices gain UV; scene objects accept a `material` with
`base_color`, optional `texture` (`gfx.texture` handle from GLB or item file), and
`unlit`. Bounded `image` decoding (PNG/JPEG), size cap, sRGB. Shader samples base colour ×
texture. Built-in cube and `gfx.mesh` keep working without UVs. E2 passes.
*Expert: runtime.*

**Step 6 — hull colliders.** Body recipe accepts `hull = mesh` (dynamic allowed); Rapier
convex hull from the mesh's positions, failure is a recipe error. Native tests for mass and
contact.
*Expert: app-host, architect.*

**Step 7 — moon rover from GLB.** `glb_scene.lua` (Lua) maps node names/extras to bodies,
wheels and visuals; the rover app swaps its generated terrain/art for fixture GLBs. E3 and E4
pass. `lua-apps.md`, `architecture.md`, `status.md` updated.
*Expert: lua-app.*

## 3. Open questions

- Hierarchy: does `gfx.scene3d` gain parented objects, or does Lua flatten node transforms
  into world space? Lean: Lua flattens (a small shared helper); revisit if per-frame cost shows.
- Sync: binary files ride the existing item sync later; this plan stores them locally only.
