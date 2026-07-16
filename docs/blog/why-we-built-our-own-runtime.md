# Why we built our own UI runtime

We didn't set out to write a GUI framework. We tried two existing ones first,
and both broke — for different reasons — at the same place: the point where a
human stops writing the UI and something else does.

## The requirement that broke everything

Our apps aren't hand-written Rust screens. They're authored in Lua, often by
an LLM agent working over MCP — you describe an app, the agent writes the
script, the runtime hot-reloads it. The tree of what's on screen has to be
buildable at runtime, from an interpreted language, by something that isn't a
Rust compiler. That's not a normal UI framework requirement. Most frameworks
assume the opposite: a person writes typed Rust, once, and the compiler is in
the loop.

## Round one: egui

We started with egui, because it's immediate-mode — you rebuild the widget
tree every frame from whatever data you have, which sounded exactly like
"rebuild the tree every frame from whatever a Lua table just produced." We
built a real Lua binding layer on top of it (`app_engine`), and it worked: you
could write `.lua` files and they became real, interactive apps, scriptable
end to end.

Then two things stopped us. First, text: egui's text stack has no support for
Indic scripts — no shaping or reordering for conjunct scripts — which was a
dead end for us. Second, and more telling: when we rotated text, the ordinary
case of an axis label on a chart, the glyphs smudged. egui caches glyphs into
a flat texture atlas; that holds up while the text sits still and falls apart
the moment you rotate it, because you're rotating a bitmap, not a shape. Both
problems trace back to the same root cause — egui's text is raster, not
vector — and neither is fixable from the app layer. You can't work around
your renderer's font shaper from Lua.

There was a second cost too, quieter but real: making Lua-authored, dynamic
UI work well on top of egui meant porting and re-implementing a lot of egui's
own internals inside `app_engine`. It worked, but "it worked" meant fighting
the framework's assumptions the whole way rather than using them.

So egui had the right shape — immediate mode, rebuild-from-data — and the
wrong renderer underneath it.

## Round two: Iced

Iced looked like the fix. It's more structured, Elm-architecture, the same
state → view → message shape we'd already settled on conceptually. But Elm
architecture and immediate-mode rebuilds aren't the same thing: Iced's
widgets are retained and built as a statically-typed Rust tree —
`Element<Message>`, generic over your app's message type, resolved at
compile time. That's a great fit if a person is writing the app in Rust. It's
the wrong fit if the tree has to come from an interpreted Lua script an agent
just wrote thirty seconds ago — there's no compile-time `Message` type to be
generic over, no static tree to build ahead of time. We spent real effort
trying to make Iced's construction path dynamic enough to be driven by Lua,
and it fought us at every layer, for the mirror-image reason egui didn't:
Iced assumes the author is the Rust compiler.

## What we actually needed

Two failures, same gap. We didn't need a UI framework — a framework, by
design, has opinions about who builds the tree and when. We needed a
rendering substrate with no opinion about that at all, so a hand-written Rust
screen and a Lua-authored app (possibly written by an LLM, possibly
hot-reloaded ten seconds ago) build the exact same tree, through the exact
same layout and paint pipeline — and neither one is the "real" way with the
other bolted on as a workaround.

That's what `runtime` is built on: winit for the window and event loop, wgpu
for the GPU, vello for painting (a real vector renderer — rotate a shape and
it's still a shape, not a smudge), parley for text (proper shaping, Indic
included), taffy for layout. None of these have an opinion about where the
tree comes from. We kept the one idea egui got right — rebuild the
description every frame, from whatever data you have — and stopped asking
the renderer to also be the framework. Lua apps and Rust screens both build
an `El` tree through the same builder vocabulary; the pipeline underneath
doesn't know or care which one authored it.

We didn't replace one framework with another. We went one level down, to
primitives that don't get in the way of the one property that actually
mattered: it has to be the same pixels, whether a person wrote the screen or
an agent just generated it.
