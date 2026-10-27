local P = require("pilot_art")
local Fx = require("fx_art")
local A = {}

-- Pilots draw through pilot_art.lua (shared with the pilot editor's preview); a soldier firing, and the
-- shots, casings and explosions, through fx_art.lua (design/jetpack-militia.md step 3).
A.actor, A.anchor = P.actor, P.anchor
A.soldier, A.effects = Fx.soldier, Fx.effects
return A
