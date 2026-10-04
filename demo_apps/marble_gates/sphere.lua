-- Unit sphere: normals and triangle winding authored here, not supplied by the engine.
return function(radius)
	local vertices, indices = {}, {}
	local rings, sides = 12, 24
	for row = 0, rings do
		local theta = math.pi * row / rings
		for col = 0, sides do
			local phi = 2 * math.pi * col / sides
			local x = math.sin(theta) * math.cos(phi)
			local y = math.cos(theta)
			local z = math.sin(theta) * math.sin(phi)
			table.insert(vertices, { position = {radius*x, radius*y, radius*z}, normal = {x,y,z} })
		end
	end
	for row = 0, rings - 1 do
		for col = 0, sides - 1 do
			local a = row * (sides + 1) + col + 1
			local b = a + sides + 1
			if row < rings - 1 then
				for _, index in ipairs({a, b+1, b}) do table.insert(indices, index) end
			end
			if row > 0 then
				for _, index in ipairs({a, a+1, b+1}) do table.insert(indices, index) end
			end
		end
	end
	return gfx.mesh({ vertices = vertices, indices = indices })
end
