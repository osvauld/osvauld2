-- PALETTE
--
-- Pure data, required by everything that draws. First file to split out precisely because it
-- depends on nothing: if `require` were broken, this is the one that would still work.
return {
	bg = "#0d1117",
	panel = "#161b22",
	card = "#1c2128",
	card_hi = "#22282f",
	sunken = "#0d1117",
	line = "#30363d",
	line_soft = "#21262d",
	text = "#e6edf3",
	muted = "#8b949e",
	accent = "#2f81f7",
	accent_hi = "#4493f8",
	danger = "#f85149",
}
