-- The shell: all of Sheprd in one view, with the sparkles over it. Pieces move
-- out of that view into mods here as they become Luau.

return mod {
	window = { titlebar = "hidden", traffic_lights = { x = 9, y = 9 } },

	init = function(previous)
		return previous or {}
	end,

	update = function() end,

	view = function()
		return ui.column {
			grow = true,
			gap = "none",
			ui.view { name = "sheprd", id = "main" },
			-- The sparkles (the built-in feedback mod): say what to change, and
			-- an agent changes this layout, asking before each edit. Above
			-- Sheprd's status bar, which fills the bottom edge.
			ui.row {
				class = "feedback-corner",
				float = { at = "bottom-right", inset = 40 },
				ui.mount "feedback",
			},
		}
	end,
}
