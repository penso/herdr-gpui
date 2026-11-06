--!strict
-- The focused workspace at a glance, in the mod panel beside its terminals:
-- where it is, its agents and what they are doing, and its tabs. Nothing
-- shows while no workspace is focused.
local ui = require("@herdr/ui")

local TONES = { blocked = "attention", working = "warning", done = "success", idle = "muted" }

return herdr.mod({
	watch = { "focus", "agents", "workspaces" },

	view = function(_, app)
		local focus = app.focus
		if not focus or not focus.workspace then
			return nil
		end

		local section = ui.column({ gap = "normal" })
		if focus.path then
			table.insert(section, ui.text({ text = focus.path, tone = "muted", size = "small", truncate = true }))
		end

		local agents = ui.list({})
		for _, agent in app.agents.all do
			if agent.host == focus.host and agent.workspace == focus.workspace then
				table.insert(agents, ui.row({ gap = "small", align = "center",
					ui.text({ text = agent.name, truncate = true }),
					ui.spacer(),
					ui.badge({ text = agent.status, tone = TONES[agent.status] or "muted" }),
				}))
			end
		end
		table.insert(section, ui.text({ text = "Agents", tone = "muted", size = "small" }))
		if #agents == 0 then
			table.insert(section, ui.text({ text = "No agents", tone = "muted" }))
		else
			table.insert(section, agents)
		end

		for _, workspace in app.workspaces do
			if workspace.host == focus.host and workspace.id == focus.workspace then
				local tabs = ui.list({})
				for _, tab in workspace.tabs do
					table.insert(tabs, ui.text({ text = tab.name, weight = if tab.focused then "bold" else "normal", truncate = true }))
				end
				table.insert(section, ui.text({ text = "Tabs", tone = "muted", size = "small" }))
				table.insert(section, tabs)
			end
		end
		return section
	end,
})
