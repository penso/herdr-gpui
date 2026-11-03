--!strict
-- An activity bar app: hosts, workspaces, and agents at a glance, with a
-- share of agents waiting on you. Its icon opens it in the window's body.
local ui = require("@herdr/ui")

local function count(list: { any }, status: string): number
	local n = 0
	for _, agent in list do
		if agent.status == status then
			n += 1
		end
	end
	return n
end

return herdr.mod({
	watch = { "hosts", "workspaces", "agents" },

	init = function()
		return { reviewed = 0 }
	end,

	update = function(state, msg)
		if msg == "command:refresh" or msg == "reviewed" then
			state.reviewed = herdr.now() or 0
		end
		return state
	end,

	view = function(state, app)
		local all = app.agents.all
		local waiting = #app.agents.blocked
		local hosts = ui.row({ gap = "small" })
		for _, host in app.hosts do
			table.insert(hosts, ui.badge({ text = host.name, tone = if host.connected then "success" else "muted" }))
		end

		local workspaces = ui.list({})
		for _, workspace in app.workspaces do
			table.insert(workspaces, ui.row({ gap = "small", align = "center",
				ui.icon({ name = "terminal", tone = if workspace.focused then "accent" else "muted" }),
				ui.text({ text = workspace.name, weight = if workspace.focused then "bold" else "normal", truncate = true }),
				ui.spacer(),
				ui.badge({ text = #workspace.tabs .. (if #workspace.tabs == 1 then " tab" else " tabs"), tone = "muted" }),
			}))
		end

		return ui.column({ gap = "large",
			ui.column({ gap = "small",
				ui.text({ text = "Hosts", tone = "muted", size = "small" }),
				hosts,
			}),
			ui.row({ gap = "large",
				ui.column({ gap = "small",
					ui.text({ text = tostring(#all), size = "large", weight = "bold" }),
					ui.text({ text = "agents", tone = "muted", size = "small" }),
				}),
				ui.column({ gap = "small",
					ui.text({ text = tostring(waiting), size = "large", weight = "bold", tone = if waiting > 0 then "attention" else "normal" }),
					ui.text({ text = "waiting on you", tone = "muted", size = "small" }),
				}),
				ui.column({ gap = "small",
					ui.text({ text = tostring(count(all, "working")), size = "large", weight = "bold", tone = "warning" }),
					ui.text({ text = "working", tone = "muted", size = "small" }),
				}),
			}),
			ui.meter({ value = if #all > 0 then waiting / #all else 0, label = "Share of agents waiting on you", tone = "attention" }),
			ui.column({ gap = "small",
				ui.text({ text = "Workspaces", tone = "muted", size = "small" }),
				workspaces,
			}),
			ui.row({ gap = "small", align = "center",
				ui.button({ label = "Mark reviewed", msg = "reviewed", tone = "accent" }),
				ui.button({ label = "New workspace", command = "new_workspace" }),
				ui.spacer(),
				ui.link({ label = "Writing mods", url = "https://github.com/penso/herdr-gpui/blob/main/docs/mods.md" }),
			}),
		})
	end,
})
