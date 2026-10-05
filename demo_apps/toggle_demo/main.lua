-- The toggle asset as a widget in an app that is not its editor: a plain frame, no world.
local Toggle = require("toggle")

local theme = Toggle.new(asset:open("toggle"))

return function()
	return ui.col({
		pad = 24,
		gap = 12,
		ui.text({ "Theme", font_size = 18, no_wrap = true }),
		Toggle.view(theme, "toggle"),
		ui.text({ "toggle: " .. Toggle.label(theme), no_wrap = true }),
	})
end
