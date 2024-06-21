barista
===============================================================================

[pista](https://github.com/xandkar/pista)'s more featureful successor.

Intended for use with [dwm](https://dwm.suckless.org/), but can just as well be
adopted to anything else with a textual status area (like
[tmux](https://github.com/tmux/)).

Runs N shell commands (specified in configuration file), asynchronously reads
lines from them and routes to N corresponding slots on the bar:

    b +--------------+--------------+- ... ---+-------------+
    a | slot_1       | slot_2       |         | slot_N      |
    r +--------------+--------------+- ... ---+-------------+
        ^              ^                        ^
        |              |                        |
       +-+            +-+                      +-+
       | |            | |                      | |
       |S|            |S|                      |S|
       |T|            |T|                      |T|
       |D|            |D|                      |D|
       |O|            |O|                      |O|
       |U|            |U|                      |U|
       |T|            |T|                      |T|
       | |            | |                      | |
       |1|            |2|                      |N|
       | |            | |                      | |
       +-+            +-+                      +-+
        ^              ^                        ^
        |              |                        |
    command_1      command_2           ...  command_N

Each slot is given a TTL, after which, if there was no update, the slot is
cleared - helping you spot broken commands and not fool yourself with stale
data.

Each command's `stderr` is redirected to `~/.barista/feeds/$i-$name/log`.

Install
-------------------------------------------------------------------------------

1. `cargo install barista --git https://github.com/xandkar/barista`
2. Ensure `~/.cargo/bin/` is in your `PATH`

Use
-------------------------------------------------------------------------------

### Initial

1. In terminal A, run `barista server`, which will create `~/.barista/`
   directory and initialize default configuration
2. In terminal B, edit configuration file to your liking: `~/.barista/conf.toml`
3. In terminal B, run `barista reload`
4. See `barista help` for more functionality

### Normal

Normally you'd run `barista server &` from `~/.xinitrc` (or similar). This will
run the commands you specified in config and set the bar at intervals you
specified in config.

While the barista server is running, you can ask it for changes and status from
any other terminal (they communicate via a Unix domain socket:
`~/.barista/socket`):

1. `barista reload` to reload configuration after changing it at runtime
2. `barista status` to see how each command is doing (last update, etc)
3. `barista off` to stop the commands and clear the bar
4. `barista on` to start the commands and start updating the bar
