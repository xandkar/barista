barista
===============================================================================

![screenshot](screenshot.png)

[![test status](https://github.com/xandkar/barista/actions/workflows/test.yml/badge.svg)](https://github.com/xandkar/barista/actions)
[![dependencies status](https://deps.rs/repo/github/xandkar/barista/status.svg)](https://deps.rs/repo/github/xandkar/barista)

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
        |              |                        |
        v              v                        v
       +-+            +-+                      +-+
       | |            | |                      | |
       |S|            |S|                      |S|
       |T|            |T|                      |T|
       |D|            |D|                      |D|
       |E|            |E|                      |E|
       |R|            |R|                      |R|
       |R|            |R|                      |R|
       | |            | |                      | |
       |1|            |2|                      |N|
       | |            | |                      | |
       +-+            +-+                      +-+
        |              |                        |
        v              v                        v
    l +--------+     +--------+     ...       +--------+
    o | file_1 |     | file_2 |               | file_N |
    g +--------+     +--------+     ...       +--------+

Each slot is given a TTL, after which, if there was no update, the slot is
cleared - helping you spot broken commands and not fool yourself with stale
data.

Each command's `stderr` is redirected to `~/.barista/feeds/$i-$name/log`.

Install
-------------------------------------------------------------------------------

1. `cargo install barista`
2. Ensure `~/.cargo/bin/` is in your `PATH`
3. `barista help`

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

Examples
-------------------------------------------------------------------------------

### Status

Yo Dawg, we heard you like status, so you can now check status of your status[^1]:

```sh
$ barista status
 POSITION  NAME        DIR                                      LAST_OUTPUTTED     LAST_LOGGED        LOG_SIZE  LOG_LINES  PID    PROC_STATE  PROC_DESCENDANTS
 0         upower      /home/xand/.barista/feeds/00-upower      7s                 3h 48m 35s         565.3 KB  6880       17424  S           17451:Z,17452:S
 1         eth         /home/xand/.barista/feeds/01-eth         0s                 8days 18h 37m 30s  4.1 MB    39819      17425  S           -
 2         wifi        /home/xand/.barista/feeds/02-wifi        0s                 8days 18h 37m 30s  4.1 MB    40508      17426  S           -
 3         bluetooth   /home/xand/.barista/feeds/03-bluetooth   0s                 8days 18h 37m 30s  4.1 MB    39537      17427  S           -
 4         memory      /home/xand/.barista/feeds/04-memory      0s                 8days 18h 37m 30s  8.2 MB    78508      17428  S           -
 5         disk        /home/xand/.barista/feeds/05-disk        3s                 8days 18h 37m 30s  1.7 MB    16210      17429  S           -
 6         backlight   /home/xand/.barista/feeds/06-backlight   4days 23h 20m 21s  8days 18h 37m 30s  39.0 KB   608        17430  S           -
 7         pulseaudio  /home/xand/.barista/feeds/07-pulseaudio  9m 16s             8days 18h 37m 30s  185.9 KB  2114       17431  S           17461:S
 8         mpd         /home/xand/.barista/feeds/08-mpd         0s                 8days 18h 37m 30s  7.9 MB    76463      17432  S           -
 9         weather     /home/xand/.barista/feeds/09-weather     15m 30s            35m 30s            185.5 KB  3146       17433  S           -
 10        time        /home/xand/.barista/feeds/10-time        0s                 8days 18h 37m 30s  10.6 MB   78515      17434  S           -
 11        keymap      /home/xand/.barista/feeds/11-keymap      0s                 8days 18h 37m 30s  8.2 MB    78491      17436  S           -
```

Footnotes
-------------------------------------------------------------------------------
[^1]: <https://en.wikipedia.org/wiki/Quis_custodiet_ipsos_custodes%3F>
