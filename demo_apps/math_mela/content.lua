local M = {}

M.kinds = { "choose", "numeric", "order" }

M.kind_meta = {
	choose = { label = "Pick a path", short = "Choose", glyph = "A B C" },
	numeric = { label = "Solve it", short = "Solve", glyph = "1 2 3" },
	order = { label = "Move the maths", short = "Hands-on", glyph = "↔" },
}

M.exercises = {
	[1] = {
		choose = {
			id = "c1-choose-seven-guavas",
			title = "Guava baskets",
			prompt = "Meena needs exactly 7 guavas. Which basket should she take?",
			hint = "Count slowly from 1. Stop when you reach 7.",
			explanation = "The basket marked 7 has exactly seven guavas.",
			options = {
				{ id = "five", label = "5 guavas" },
				{ id = "seven", label = "7 guavas" },
				{ id = "nine", label = "9 guavas" },
			},
			answer = "seven",
		},
		numeric = {
			id = "c1-solve-laddus",
			title = "Laddus on a plate",
			prompt = "There are 6 laddus. Amma adds 3 more. How many are there now?",
			equation = "6 + 3 = ?",
			hint = "Start at 6 and count 3 more: 7, 8, ...",
			explanation = "Counting three steps after 6 lands on 9, so 6 + 3 = 9.",
			answer = 9,
			suffix = "laddus",
		},
		order = {
			id = "c1-order-number-cards",
			title = "Number card parade",
			prompt = "Drag the marker to visit 2, then 5, then 8 on the number line.",
			hint = "Find the card closest to zero first.",
			explanation = "2 comes before 5, and 5 comes before 8 on the number line.",
			items = {
				{ id = "eight", label = "8" },
				{ id = "two", label = "2" },
				{ id = "five", label = "5" },
			},
			answer = { "two", "five", "eight" },
			manip = { min = 0, max = 10, targets = { 2, 5, 8 }, labels = { "2", "5", "8" }, unit = "" },
		},
	},
	[2] = {
		choose = {
			id = "c2-choose-square",
			title = "Rangoli shapes",
			prompt = "Which shape has 4 equal sides and 4 corners?",
			hint = "Look for four sides that are all the same length.",
			explanation = "A square has four equal sides and four corners.",
			options = {
				{ id = "triangle", label = "Triangle · 3 sides" },
				{ id = "square", label = "Square · 4 equal sides" },
				{ id = "rectangle", label = "Rectangle · 2 long + 2 short" },
			},
			answer = "square",
		},
		numeric = {
			id = "c2-solve-library-books",
			title = "Library shelf",
			prompt = "One shelf has 34 books and another has 28. How many books altogether?",
			equation = "34 + 28 = ?",
			hint = "Add the ones first: 4 + 8 = 12. Carry one ten.",
			explanation = "34 + 28 = 62: twelve ones become 1 ten and 2 ones.",
			answer = 62,
			suffix = "books",
		},
		order = {
			id = "c2-order-lengths",
			title = "Tailor's measuring tape",
			prompt = "Drag along the tape to visit 25 cm, 60 cm, then 1 m.",
			hint = "Remember: 1 metre is 100 centimetres.",
			explanation = "25 cm is shortest, then 60 cm, and 1 m equals 100 cm.",
			items = {
				{ id = "one-m", label = "1 m" },
				{ id = "twenty-five", label = "25 cm" },
				{ id = "sixty", label = "60 cm" },
			},
			answer = { "twenty-five", "sixty", "one-m" },
			manip = { min = 0, max = 100, targets = { 25, 60, 100 }, labels = { "25 cm", "60 cm", "1 m" }, unit = " cm" },
		},
	},
	[3] = {
		choose = {
			id = "c3-choose-quarter",
			title = "Sharing a roti",
			prompt = "A roti is cut into 4 equal pieces. Riya takes 1 piece. What fraction did she take?",
			hint = "The bottom number tells how many equal pieces there are.",
			explanation = "One of four equal pieces is one quarter, written 1/4.",
			options = {
				{ id = "half", label = "1/2" },
				{ id = "quarter", label = "1/4" },
				{ id = "three-fourths", label = "3/4" },
			},
			answer = "quarter",
		},
		numeric = {
			id = "c3-solve-mango-crates",
			title = "Mango crates",
			prompt = "A stall has 7 crates with 6 mangoes in each. How many mangoes is that?",
			equation = "7 × 6 = ?",
			hint = "Add seven groups of 6, or use the 6 times table.",
			explanation = "Seven equal groups of six make 42 mangoes.",
			answer = 42,
			suffix = "mangoes",
		},
		order = {
			id = "c3-order-place-value",
			title = "Build 4,582",
			prompt = "Tap each place-value wheel until the display builds 4,582.",
			hint = "Thousands come before hundreds, tens, and ones.",
			explanation = "4 thousands + 5 hundreds + 8 tens + 2 ones makes 4,582.",
			items = {
				{ id = "tens", label = "8 tens" },
				{ id = "thousands", label = "4 thousands" },
				{ id = "ones", label = "2 ones" },
				{ id = "hundreds", label = "5 hundreds" },
			},
			answer = { "thousands", "hundreds", "tens", "ones" },
			manip = { kind = "place", digits = { 4, 5, 8, 2 } },
		},
	},
	[4] = {
		choose = {
			id = "c4-choose-equivalent-fraction",
			title = "Equal fraction flags",
			prompt = "Which fraction has the same value as 3/4?",
			hint = "Multiply the top and bottom of 3/4 by the same number.",
			explanation = "Multiplying both 3 and 4 by 2 gives 6/8, so the fractions are equal.",
			options = {
				{ id = "six-eighths", label = "6/8" },
				{ id = "four-sixths", label = "4/6" },
				{ id = "nine-sixteenths", label = "9/16" },
			},
			answer = "six-eighths",
		},
		numeric = {
			id = "c4-solve-stationery",
			title = "Stationery bill",
			prompt = "Three notebooks cost ₹48 each. One pen costs ₹26. What is the total?",
			equation = "3 × 48 + 26 = ?",
			hint = "Find the notebook cost first, then add the pen.",
			explanation = "3 × 48 = 144, and 144 + 26 = 170.",
			answer = 170,
			suffix = "rupees",
		},
		order = {
			id = "c4-order-angles",
			title = "Angle line-up",
			prompt = "Drag the angle marker to visit 35°, 90°, 120°, then 180°.",
			hint = "An acute angle is below 90°. A straight angle is 180°.",
			explanation = "35° < 90° < 120° < 180°.",
			items = {
				{ id = "obtuse", label = "120°" },
				{ id = "straight", label = "180°" },
				{ id = "acute", label = "35°" },
				{ id = "right", label = "90°" },
			},
			answer = { "acute", "right", "obtuse", "straight" },
			manip = { min = 0, max = 180, targets = { 35, 90, 120, 180 }, labels = { "35°", "90°", "120°", "180°" }, unit = "°" },
		},
	},
	[5] = {
		choose = {
			id = "c5-choose-decimal",
			title = "Decimal match",
			prompt = "Which decimal is equal to three quarters (3/4)?",
			hint = "Think of 3/4 as 75 out of 100.",
			explanation = "3/4 = 75/100, and seventy-five hundredths is 0.75.",
			options = {
				{ id = "zero-three-four", label = "0.34" },
				{ id = "zero-seven-five", label = "0.75" },
				{ id = "one-two-five", label = "1.25" },
			},
			answer = "zero-seven-five",
		},
		numeric = {
			id = "c5-solve-train-distance",
			title = "Train journey",
			prompt = "A route is 2,450 km long. The train has covered 875 km. How far remains?",
			equation = "2,450 − 875 = ?",
			hint = "Subtract 800, then 75 more.",
			explanation = "2,450 − 875 = 1,575 kilometres remaining.",
			answer = 1575,
			suffix = "kilometres",
		},
		order = {
			id = "c5-order-operations",
			title = "Operation pathway",
			prompt = "Shade rows of 6: make 6 × 4 first, then add 18 as 3 more rows.",
			hint = "Multiplication comes before addition.",
			explanation = "Multiply 6 × 4 first to get 24, then add 18 to get 42.",
			items = {
				{ id = "finish", label = "Write the answer 42" },
				{ id = "multiply", label = "Multiply 6 × 4 = 24" },
				{ id = "add", label = "Add 18 + 24" },
			},
			answer = { "multiply", "add", "finish" },
			manip = { kind = "grid", rows = 7, cols = 6, split = 4 },
		},
	},
}

function M.get(class_no, kind)
	return M.exercises[class_no][kind]
end

function M.item(exercise, id)
	for _, item in ipairs(exercise.items or {}) do
		if item.id == id then
			return item
		end
	end
end

return M
