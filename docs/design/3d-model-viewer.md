# Interactive 3D models — Lua-first rendering proof

Status: **implementation started, 2026-09-20.** The first slice renders bounded built-in cubes
from strict Lua scene declarations through an owned WGPU depth pass into live/custom captures;
`DumpTree` exposes the validated camera and object data. The proof now has 4× MSAA, face normals,
simple directional lighting, Lua-owned drag orbit/wheel zoom, and CPU ray picking of transformed
cubes through the normal click route. It is still one viewport and its resolved 3D overlay
composites after the complete Vello scene, so hover picking, foreground overlay composition and
GLB remain unbuilt. Blender is an authoring tool, not an embedded application or
runtime dependency. This plan narrows the first proof
of the [Environment runtime](environment-runtime.md); it does not freeze a public Lua API
or select a full game engine.

## 1. Goal

Import models authored in Blender (or another glTF exporter), display them inside Osvauld,
and let Lua apps control and interact with them:

```text
Blender → GLB export → encrypted asset → native 3D scene → viewport in a Lua app
                                            ↑
                                  Lua commands and events
```

GLB is the initial target: meshes, supported materials, textures, hierarchy and eventually
animation clips. Blender modifiers, arbitrary shader graphs, Geometry Nodes and simulations
are not runtime behaviors; they need baking or conversion where supported. Imported assets
must never execute embedded scripts.

Frame remains 2D. A separate retained 3D scene owns cameras, meshes, transforms and depth;
a normal UI viewport places its output alongside existing controls.

## 2. Capability checklist

### Core viewer

- GLB parsing and validation: mesh attributes/indices, node hierarchy, materials and textures.
- Binary asset storage: encrypted bytes, stable references, explicit loading/cache lifetime.
- Retained model instances with stable, app-scoped identities and parent/local transforms.
- Defined coordinate conventions, units, position/scale, quaternion rotation and camera matrices.
- Perspective and orthographic cameras; orbit, pan and zoom interaction.
- GPU mesh buffers, depth testing, texture sampling and a documented material subset.
- Basic lighting; environment lighting and shadows can follow the first rendering proof.
- A viewport in ordinary Lua layout, respecting outer transforms, clips, controls and overlays.

### Lua interaction

- Load resources, instantiate models, manipulate transforms and supported material properties.
- Camera control and bounded plain-data input events; no Lua VM objects in runtime state.
- Pointer-to-ray picking, nearest visible-part selection and highlighting.
- Stable imported-part identity; duplicate or absent Blender node names cannot be the sole key.
- Optional manipulation gizmos after basic selection and app-driven transforms work.
- Imported animation playback later, with skeletal skinning and morph targets explicitly scoped.

### Runtime and product foundations

- Worlds/resources survive ordinary view rebuilds; define tab, reload and teardown behavior.
- App-scoped, generation-checked handles; reject stale and cross-app resource use.
- Repaint while active and sleep while idle; add fixed-step scheduling only for simulation.
- Persist asset references and authored choices in documents, not hover/camera gestures or
  every animation tick. Camera persistence is an explicit app choice.
- Bound input bytes, decoded texture sizes, vertices, instances, hierarchy depth and GPU memory.
- Reject unsupported or malformed assets clearly; no unrestricted external file/network paths.
- Capture the final 2D/3D composite through existing live and custom bridge screenshots.
- Bounded scene inspection and normal-pipeline automation for reproducible interaction tests.

### Later capabilities, not prerequisites

Physics/colliders/joints; shadows and advanced transparency; post-processing; instancing,
culling and level of detail; procedural meshes; UI projected onto world surfaces; cloth,
ropes and other deformables. Asset synchronization follows the separate workspace sync design.

## 3. First proof: a real Lua app

The deliverable is a demo app loaded into the existing shell, **not a standalone Rust demo**.
Rust provides the smallest generic rendering/picking support; Lua owns scene composition and
behavior through experimental bindings. Proposed API names must be labeled experimental.

The app must:
1. Put a 3D viewport beside ordinary Lua controls and a selected-object label.
2. Display two overlapping, depth-intersecting built-in objects with distinguishable colors.
3. Control camera and object rotation from Lua; exercise orbit/zoom input.
4. Pick an object, highlight it and update the ordinary UI label.

Built-in geometry deliberately avoids GLB parsing in this proof. GLB loading is the next
independent milestone, not evidence required to establish correct compositing and depth.

Acceptance:
- Depth is correct regardless of declaration order; rotating the objects changes visible
  intersections correctly.
- Rendering shares the existing WGPU device/window/event loop; no CPU texture readback between
  rendering passes. Final screenshot readback is allowed.
- Viewport clipping and outer UI eligibility work; overlays never click through to the scene.
- Picking agrees with visible depth for the opaque proof objects and reports stable identities.
- A bridge screenshot contains the complete UI and 3D result. Live and custom capture paths
  both use the final composite.
- Input tests exercise normal routing, not a special path that bypasses viewport eligibility.
- Static scenes stop requesting redraws. Prefer event-driven controls initially; any continuous
  animation must first pin the scheduling semantics described in Environment Gate 0.

This proves mesh/depth composition and basic picking only. It does **not** prove projected
Taffy/Vello controls, deformable surfaces, production world lifetime, PBR fidelity or physics.

## 4. Selected substrate and dependencies

**Decision, 2026-09-20:** Osvauld owns the WGPU render passes and composition. Bevy is not a
runtime dependency and its coupled `bevy_render`/`bevy_pbr`/`bevy_gltf` crates are not the
implementation substrate. This uses the device, event loop, geometry, input and screenshot
pipelines we already control. Bevy remains a reference and feature benchmark.

Use focused libraries rather than rebuilding solved subsystems:

| concern | selected candidate |
|---|---|
| GPU, shader translation, window | existing WGPU 29/Naga and winit 0.30 |
| vector UI and layout | existing Vello, Parley and Taffy |
| 3D math and GPU data | `glam`; `bytemuck`, with `encase` evaluated for uniforms |
| GLB/glTF and images | `gltf` 1.4 and bounded `image` decoding |
| tangents and mesh optimization | `mikktspace`; `meshopt` when the corresponding tier lands |
| picking and geometry queries | `parry3d` |
| optional rigid physics | direct `rapier2d`/`rapier3d` 0.35, as separate world backends |
| skeletal runtime and compressed textures | evaluate `ozz-animation-rs` and KTX2/Basis later |

The host owns pass ordering, targets, cameras, GPU resource caches, viewport clipping,
Vello/3D/overlay composition, final screenshots, Lua-to-native commands, app-scoped handles and
budgets. It does not implement file/image parsing, matrix math, collision algorithms, rigid-body
solving, tangent generation or mesh optimization. Rapier is physics, not rendering; the first
proof has no physics, and may use Parry only when picking lands. No ECS is selected: begin with
focused retained stores and do not expose their layout as the public World contract.

`rend3` 0.3 is rejected because it uses WGPU 0.12. `three-d` is OpenGL/WebGL-oriented. Fyrox and
Bevy bring application/rendering architectures that compete with the live runtime. Record a dated
revision before reversing this decision; dependency compatibility and maintenance must still be
rechecked when each later tier begins.

## 5. Sequence

1. Lua rendering proof above: shared final-composite path, depth target, built-in mesh pass,
   viewport, experimental Lua controls, then picking and screenshot verification.
2. One GLB model with a documented material subset, basic lighting and orbit/pan/zoom.
3. Imported-part picking/highlighting and Lua transform/material controls.
4. Product integration: encrypted assets, persistence, lifecycle/reload, limits and automation.
5. Imported animation, then physics or spatial UI according to real app needs.

Every experimental slice still validates its inputs and bounds allocations; step 4 is not
permission to defer safety. Temporary asset delivery for step 2 needs an explicit scoped design.
Before implementation, inspect current renderer/device/screenshot ownership, load the matching
expert skills, split work into approximately 100-line code slices and get user approval.
Document exact supported features and remaining limitations as each slice lands.
