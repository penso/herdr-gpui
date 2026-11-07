# Mods that could not run or be listed. $name is a mod's name, as its
# mod.toml gives it; `[mods] disabled` is a config table and key.

error-mods-busy = Mod { $name } has too many messages waiting; try again
error-mods-disabled-name = `[mods] disabled` has an invalid name `{ $name }`
error-mods-not-running = No mod named { $name } is running
error-mods-spawn = Could not start a thread for mod { $name }
# $limit is the most mods the list may name.
error-mods-too-many-disabled = `[mods] disabled` lists more than { $limit } mods
