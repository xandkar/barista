barista
===============================================================================

Runs N shell commands (specified in configuration file), asynchronously reads
lines from them and routes to N corresponding slots on the bar:

    b +--------------+--------------+- ... ---+-------------+
    a | slot_1       | slot_2       |         | slot_N      |
    r +--------------+--------------+- ... ---+-------------+
        ^              ^                        ^
        |              |                        |
    command_1      command_2           ...  command_N

Each slot is given a TTL, after which, if there was no update - the slot is
cleared - helping you spot broken status commands and not fool yourself with
stale data.

Install
-------------------------------------------------------------------------------

    cargo install barista --git https://github.com/xandkar/barista

Use
-------------------------------------------------------------------------------

1. In terminal A, run `barista server`, which will create `~/.barista/`
   directory and initialize default configuration
2. In terminal B, edit configuration file to your liking: `~/.barista/conf.toml`
3. In terminal B, run `barista reload`
4. See `barista help` for more functionality
