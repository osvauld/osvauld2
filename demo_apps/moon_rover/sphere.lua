return function(radius)
	local vertices, indices = {}, {}
	local rings, sides = 8, 16
	for row = 0, rings do
		local theta = math.pi * row / rings
		for col = 0, sides do
			local phi = 2 * math.pi * col / sides
			local x, y, z = math.sin(theta) * math.cos(phi), math.cos(theta), math.sin(theta) * math.sin(phi)
			vertices[#vertices + 1] = {position = {radius*x, radius*y, radius*z}, normal = {x,y,z}}
		end
	end
	for row = 0, rings - 1 do
		for col = 0, sides - 1 do
			local a = row * (sides + 1) + col + 1
			local b = a + sides + 1
			if row < rings - 1 then
				for _, index in ipairs({a, b+1, b}) do indices[#indices + 1] = index end
			end
			if row > 0 then
				for _, index in ipairs({a, a+1, b+1}) do indices[#indices + 1] = index end
			end
		end
	end
	return gfx.mesh({vertices = vertices, indices = indices})
end
