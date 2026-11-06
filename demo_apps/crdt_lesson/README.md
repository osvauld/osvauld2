# Authoring an interactive lesson

This app teaches a state-based grow-only counter. It is an algorithm simulation with two fixed
replica IDs, not Loro or a network client. `lesson.lua` owns the narration and demonstration cues;
`model.lua` computes the results; `visuals.lua` renders them. The suggested timings are rehearsal
defaults. The presenter supplies the voice.

The subject-neutral library is `lib/presenter.lua`, API version 1, with `lib/theme.lua`.
Bundle both files into another app: sandboxed `require` resolves only inside that app's source.
The library supports one presenter per app; its stable UI IDs are also its automation surface.
SVG artwork can later be implemented inside a subject's `view` without changing this contract.

```lua
local presenter = require("lib/presenter")

return presenter.app({
	id = "my-lesson",
	title = "My subject",
	scenes = {
		{
			id = "introduction", short = "Start", title = "A useful question",
			summary = "The point this scene makes.",
			narration = "The words the presenter will say.",
			cue = "What to demonstrate, and where to pause.",
			seconds = 20,
			cues = { { at = 1, action = "reveal", label = "Reveal the answer" } },
		},
	},
	prepare = function(scene_id) return { revealed = false } end,
	actions = { reveal = function(model) model.revealed = true end },
	view = function(model, context)
		return ui.button({
			id = "reveal-answer",
			on_click = function() context.act("reveal") end,
			ui.text({ model.revealed and "The answer" or "Try it" }),
		})
	end,
})
```

`prepare(scene_id)` returns fresh plain viewer state; it must not perform host or document
actions. Actions mutate only that model. `view` describes UI without modifying state. Manual
controls call `context.act(id)`; timed cues call the same declared action. Cue times must be
ordered, within the scene duration, and reference existing actions. Scene IDs are unique.

`context` also contains `mode`, `scene`, `playing`, `last_action`, and `motion` (0–1 progress
since the latest timed action, over 0.8 seconds). Arbitrary Lua views are supported; diagrams
are optional. Hide subject controls when `context.mode == "record"`.

Explore permits experiments. Play starts the prepared demonstration, Pause preserves time,
and manual actions stop it; playing after a manual experiment starts the prepared scene again.
Previous, Next, scene tabs and Reset prepare a fresh model. Mode changes pause without changing
the model. Viewer state survives hot reload, but is not persisted or shared in app documents.
Changing model shape may require Reset scene after a reload.

Presenter shows the narration and cue sheet. Record view hides presentation controls and notes.
Keyboard: Space plays/pauses, arrows change scenes, Escape returns to Presenter. Navigation can
also use the `scene:<id>`, `mode:presenter`, `lesson-play`, and `lesson-reset` UI IDs.

Use `scripts/present_lesson.py --script PATH` to export notes by reading the actual rendered
presenter. `--video PATH --preview 3.5` records a short silent preview of each scene; omit
`--preview` for the declared scene durations. `--open` leaves a visible demo window running.
Run through LoadTable with a built `OSVAULD_SHELL_BINARY` and the display environment. The
script works for another folder implementing this same presenter contract.

Counter reference: [Shapiro et al. (2011), §3.1.2](https://www.lri.fr/~mbl/ENS/CSCW/2021/papers/CRDT-study11.pdf).
It covers increments only; text editing, deletion and decrements need other designs.
