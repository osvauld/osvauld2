local presenter = require("lib/presenter")
local lesson = require("lesson")
local model = require("model")
local visuals = require("visuals")

return presenter.app({
	id = "crdt-counter", title = lesson.title, scenes = lesson.scenes,
	prepare = model.prepare, actions = model.actions, view = visuals.view,
})
