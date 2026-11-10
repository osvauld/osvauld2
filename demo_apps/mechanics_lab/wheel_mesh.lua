return function(radius,width,segments)
	local vertices,indices={},{}
	local function triangle(a,b,c,normal)
		for _,point in ipairs({a,b,c}) do
			vertices[#vertices+1]={position=point,normal=normal}
			indices[#indices+1]=#vertices
		end
	end
	for i=0,segments-1 do
		local a,b=i*2*math.pi/segments,(i+1)*2*math.pi/segments
		local ya,za,yb,zb=radius*math.cos(a),radius*math.sin(a),radius*math.cos(b),radius*math.sin(b)
		local left,right=-width/2,width/2
		local p,q,r,s={left,ya,za},{left,yb,zb},{right,ya,za},{right,yb,zb}
		local normal={0,math.cos((a+b)/2),math.sin((a+b)/2)}
		triangle(p,q,s,normal); triangle(p,s,r,normal)
		triangle({left,0,0},q,p,{-1,0,0}); triangle({right,0,0},r,s,{1,0,0})
	end
	return gfx.mesh({vertices=vertices,indices=indices})
end
