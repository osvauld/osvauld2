local C = require("theme").mission

return function(seed)
	local columns, rows = C.width/C.step+1, C.depth/C.step+1
	local heights, vertices, indices = {}, {}, {}
	local phase = (seed % 10007)/10007*math.pi*2
	local function smooth(a,b,x)
		local t = math.clamp((x-a)/(b-a),0,1)
		return t*t*(3-2*t)
	end
	local function crater(x,z,cx,cz,radius,depth)
		local r = math.sqrt((x-cx)^2+(z-cz)^2)
		local floor_radius = math.min(4,radius*0.3)
		local bowl = -depth*(1-smooth(floor_radius,radius,r))
		return bowl+0.22*math.exp(-((r-radius)/3)^2)
	end
	local function height(x,z)
		local y = 0.22*math.sin(x*0.11+phase)*math.cos(z*0.09-phase)
			+0.10*math.sin((x+z)*0.19+phase*2)
		y = y+crater(x,z,C.site[1],C.site[2],C.crater_radius,C.crater_depth)
			+crater(x,z,-27,22,8,0.9)+crater(x,z,30,-24,7,0.7)
		local home = math.sqrt((x-C.home[1])^2+(z-C.home[2])^2)
		y = y*smooth(5,8,home)
		local site = math.sqrt((x-C.site[1])^2+(z-C.site[2])^2)
		return -C.crater_depth+(y+C.crater_depth)*smooth(3,5,site)
	end
	for row=0,rows-1 do
		heights[row+1] = {}
		for col=0,columns-1 do
			heights[row+1][col+1] = height(col*C.step-C.width/2,row*C.step-C.depth/2)
		end
	end
	for row=0,rows-1 do
		for col=0,columns-1 do
			local left,right = math.max(col-1,0),math.min(col+1,columns-1)
			local back,front = math.max(row-1,0),math.min(row+1,rows-1)
			local dx = (heights[row+1][right+1]-heights[row+1][left+1])/((right-left)*C.step)
			local dz = (heights[front+1][col+1]-heights[back+1][col+1])/((front-back)*C.step)
			vertices[#vertices+1] = {position={col*C.step-C.width/2,heights[row+1][col+1],row*C.step-C.depth/2},normal={-dx,1,-dz}}
			if row<rows-1 and col<columns-1 then
				local a = row*columns+col+1
				for _,i in ipairs({a,a+columns,a+1,a+1,a+columns,a+columns+1}) do indices[#indices+1]=i end
			end
		end
	end
	local function sample(x,z)
		local u,v = (x+C.width/2)/C.step,(z+C.depth/2)/C.step
		if u<0 or v<0 or u>columns-1 or v>rows-1 then return nil end
		local col,row = math.min(math.floor(u),columns-2),math.min(math.floor(v),rows-2)
		local fx,fz = u-col,v-row
		local a,b = heights[row+1][col+1],heights[row+1][col+2]
		local c,d = heights[row+2][col+1],heights[row+2][col+2]
		if fx+fz<=1 then return a+fx*(b-a)+fz*(c-a) end
		return d+(1-fx)*(c-d)+(1-fz)*(b-d)
	end
	return {mesh=gfx.mesh({vertices=vertices,indices=indices}),sample=sample,seed=seed}
end
