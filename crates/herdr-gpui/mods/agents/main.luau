--!strict
-- Counts the agents waiting on you and those working, across every host.
-- Click the item, or run its palette command, to list the waiting agents.
local ui = require("@herdr/ui")

return herdr.mod({
	watch = { "agents" },

	init = function()
		return { names = false }
	end,

	update = function(state, msg)
		if msg == "toggle" or msg == "command:names" then
			state.names = not state.names
		end
		return state
	end,

	view = function(state, app)
		local blocked = app.agents.blocked
		local working = app.agents.working
		local row = ui.row({ gap = "small", align = "center" })
		if #blocked == 0 and #working == 0 then
			table.insert(row, ui.text({ text = "No agents busy", tone = "muted", size = "small" }))
			return row
		end
		if #blocked > 0 then
			table.insert(row, ui.badge({ text = tostring(#blocked) .. " waiting", tone = "attention" }))
		end
		if #working > 0 then
			table.insert(row, ui.badge({ text = tostring(#working) .. " working", tone = "warning" }))
		end
		if state.names and #blocked > 0 then
			local names = {}
			for _, agent in blocked do
				table.insert(names, agent.name)
			end
			table.insert(row, ui.text({ text = table.concat(names, ", "), truncate = true, size = "small" }))
		end
		table.insert(row, ui.button({ name = "chevron-right", msg = "toggle", tone = "muted" }))
		return ui.tooltip({ text = "Agents waiting on you, and working", row })
	end,
})
