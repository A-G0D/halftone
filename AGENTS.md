# halftone for agents

halftone is a full-screen terminal desk of cards. You change it by editing plain files; the running
app picks every change up live (it polls the desk file twice a second and re-runs modules on their
own interval). No restart, no build.

- Desk file: `halftone config path` (default `~/.config/halftone/desk.conf`). Check it with `halftone config check`.
- Modules dir: `~/.config/halftone/modules/` (next to the desk file).
- This file is copied next to the desk file on first run.

## Desk file

One setting per line, `#` starts a comment. A bad line shows as a note and keeps the old layout.

```
theme ink                       # a theme name (press t in the app to cycle)
agent claude                    # command the agents card opens
tab Home                        # starts a tab; rows below belong to it
row 4: media 3, apps 2          # row <height|fill [weight]>: <card> [args] [weight], ...
row fill: agents 3, watch 2x2 2
watch TSLA NVDA AAPL            # tickers for the watch card
media Music ~/a.m4a             # media <label> <file|url|ytsearch1:query> [bg <video>] [from m:ss]
connect gmail @app              # live data: discord, gmail, stripe, x (halftone setup)
key ctrl+t send cargo test\r    # key <combo> send <text> | tab <name> | run <cmd> | theme <name> | voice
mod island                      # optional features: voice lyrics youtube island toasts
```

A row is a height in lines (`4`) or `fill` (`fill 2` = double share of the leftover space). Each
card in the row has a trailing weight (its share of the width, default 1).

## Cards

- `media` now-playing from Spotify/Music, with controls.
- `apps` unread badges (Discord, Gmail, ...) from `connect` lines.
- `agents` your Claude Code and Codex sessions; Enter opens one.
- `watch [1x2|2x2|1x4|2x4]` stock tiles for the `watch` tickers; click for a chart.
- `revenue` Stripe income (sample data until `connect stripe`).
- `x` your X feed (needs `connect x`).
- `term <command>` an embedded terminal running the command (default: shell).
- `player <mode> [label]` the media player; modes video lyrics split disc.
- `<module name>` any script in the modules dir, see below. Built in: weather system github
  hackernews crypto clocks todo ports git docker.

Example: add `row 8: weather, system, hackernews` under a tab and save.

## Modules

A module is any executable file in the modules dir. Name the file, use the name as a card kind.
halftone runs it in the background every `every` seconds (default 60) with a 10 s timeout, keeps
the last good output, and shows stderr or a non-zero exit dimly under it. Write any language;
print lines to stdout:

```
title: X            card title (default: file name)
every: 30           seconds between runs (min 2)
open: <url|cmd>     Enter or click on the card: a URL goes to open/xdg-open, anything else runs in sh
connect: <name> | <url> | <what to paste>   the card needs a token first (see Connecting)
big: text           bold headline
row: label | value | tone       label left, value right; tone optional
bar: label | 0..1 | tone        thin meter with percent
spark: 1 3 2 8 5                one-line sparkline (scaled to min..max)
link: label | url               clickable row (same open rules)
text: ...           muted line (a line with no known prefix is text too)
error: ...          dim red line
```

Tones are `good bad warn muted accent`. Lines draw top to bottom and are cut at the card height, so
put the important ones first. Full example, `~/.config/halftone/modules/uptime`:

```sh
#!/bin/sh
echo "title: Uptime"
echo "every: 30"
echo "big: $(uptime | sed 's/.*up *//; s/,.*//')"
echo "row: load | $(uptime | awk -F'load averages?: ' '{print $2}' | cut -d' ' -f1) | accent"
echo "bar: disk | $(df -k / | awk 'NR==2{gsub("%","",$5); print $5/100}') | good"
echo "link: docs | https://example.com"
```

Header convention: within the script's first 5 lines, `# about: <one line>` (shown in the card
gallery, key `a`) and optionally `# group: <Finance|Mail & calendar|Dev|News|System|...>` (no group
lists under "Other").

### Connecting (tokens)

halftone runs every module with `HALFTONE` set to its own path. Read a saved token with
`"$HALFTONE" secret get <name>` (exit 1, no output, if there is none). When a module has no token yet,
print one line and nothing else useful:

```sh
tok=$("$HALFTONE" secret get todoist) || { echo "connect: todoist | https://app.todoist.com/app/settings/integrations/developer | your API token"; exit 0; }
```

The card then reads "Connect Todoist · press enter"; Enter or a click opens the page in the browser,
asks for the paste (hidden), saves it and reruns the module at once. Secrets: `halftone secret
set|get|rm <name>` (`set` reads stdin). macOS Keychain (service `halftone`); elsewhere files in
`<config dir>/secrets/` (mode 600). Never put tokens in argv, the desk file or module output.

Then `chmod +x` it, add `row 6: uptime` to a tab, save (or press `a` in the app). Built-in modules are written once and never
overwritten, so edit them freely. Settings come from env vars: `HALFTONE_CITY` (weather),
`HALFTONE_ZONES` (clocks, e.g. `Europe/London Asia/Tokyo`), `HALFTONE_REPOS` (git, root dirs).

## Commands

- `halftone module list` names, paths and last-run status.
- `halftone module new NAME` scaffold with the protocol in comments; prints the desk line to add.
- `halftone module run NAME` run once and print the parsed lines (title, every, open, then each
  line as Big/Row/Bar/Spark/Link/Text/Error). Use this to debug; a typo'd prefix shows up as `Text`.
- `halftone notify APP TITLE [BODY]` pop a toast in the running desk (needs `mod toasts`; an
  APP of Claude or Codex goes to the island when `mod island` is on). Use it to tell the user a task finished.
- `halftone config [path|check|print|reset]` the desk file.
- `halftone setup [STEP]` link stocks, discord, gmail, stripe, x (optional: the desk opens with no questions).
- `halftone secret set|get|rm NAME` module tokens; `halftone connect NAME URL [what to paste]` open a page and save the paste (`connect x` is the X sign-in).

## Working on the code

Rust, zero dependencies (shell out to curl and system tools). Desk grammar: `src/workspace/config.rs`.
Card drawing: `Workspace::widget` in `src/workspace/mod.rs`, helpers in `cards.rs`. Modules:
`src/workspace/modules.rs`, shipped scripts in `modules/` (every file there is embedded by `build.rs`, no Rust edit needed; POSIX sh). `cargo test --release`
must pass; keep each new shipped module under `sh -n` (a test checks it) and make it print a
helpful `text:` line when a tool it needs is missing.
