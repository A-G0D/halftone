# Halftone: audit and wishlist (2026-10-29)

## Tacky, and what replaced it

| Was | Now |
| --- | --- |
| Boxed cards with heavy borders | Flat cards on the panel colour, quiet titles |
| Full-block bars and meters | Thin ━ on ─ meters, braille charts and level lines |
| Saturated accents (pure red/green/brand hues) | Softened INK palette; brand colours pulled toward the theme with `tone()` |
| Big 3×5 pixel numerals | Plain text at weight; `big()` kept but unused |
| Half-block video only | Quadrant mode (2×2 per cell); real pixels over the kitty protocol where the terminal has it |
| Things popping in and out | Eased tweens, push/pop slides, gliding tab underline, toast stack settle, theme crossfade |
| Separate island process in tmux | Island drawn inside the workspace |
| Song view on its own | Player modes: video, lyrics, split, disc |
| Long subtitles on the page | Titles only, short phrases |
| Truecolor pulled 60% toward the nearest RGB slot (muddy) | Greys onto the theme's bg→text ramp, colours onto the theme hue nearest them, 85% |

## Still limited

- Cell video is capped by terminal resolution unless the terminal speaks kitty graphics (Ghostty, kitty, WezTerm). iTerm2 and sixel are not wired.
- Watchlist, revenue, agents, inbox and news are sample data. No live feeds.
- Truecolor retheme keeps 15% of the app's own colour, and an app's brand hue can land on a different theme slot (Claude's orange lands near the theme's red or sand).
- yt-dlp sources take seconds to open and depend on YouTube.
- Voice needs ffmpeg, whisper-cli and a 142 MB model.
- No mouse in the workspace yet (the VT parses mouse modes but cards ignore clicks).

## Tested in Ghostty (2026-10-30)

- Video plays as real pixels (kitty protocol, 512×288 when available): Roadster from the CNET recap at 1:58, the Starship catch.
- Live whisper lyrics land "Well, it worked." then "flying cars are real" on the flip shot.
- Claude Code inside a card picks up the theme (its orange rule and selection turn sand and blue).
- Fixed from that run: "1 agents" plural, split "(crowd cheering)" leaking into lyrics, soft upscaled video.
- kitty (the graphics protocol's reference terminal): the Home desk renders cleanly with a toast sliding in.
- Keys through a real PTY (`examples/drive.rs` with `tapes/keys.steps`): Tab focus, Enter into the TSLA detail, Esc back, `2` to Code (lazygit, vim), ctrl+k palette filtering "thm" to the themes, editor `l` move saved to the layout file, ctrl+space twice typing "Run the tests and tell me what failed." into a shell.
- Not done: typing into the Ghostty or kitty window (computer use is click-only for terminals), so tab keys, palette and editor were checked in VHS only.

## Four toolkits re-themed, hands-on (2026-10-30)

`tapes/stacks.tape` runs btop (C++), gitui (Rust, ratatui), the Textual demo (Python) and glow (Go, Bubble Tea) in one tab, then switches to paper. Found and fixed: apps started dark stayed dark slabs on paper (grey mapping now follows each app's own polarity, refreshed when it asks OSC 10/11), and paper, amber and moss used saturated Tailwind/neon hues (now muted).

## Beyond ratatui (sources checked 2026-10-30)

| Stack | What to take |
| --- | --- |
| [Notcurses](https://github.com/dankamongmen/notcurses) (C) | Picks the best blitter the terminal supports: pixels (kitty, sixel), then octant, sextant, quadrant, braille. Halftone has pixels, quadrant, braille; sextant/octant are next. |
| [OpenTUI](https://github.com/sst/opentui) (Zig + TS, runs OpenCode) | Native core with a flexbox layer; images and 3D in the terminal. Halftone's rows/weights stay simpler on purpose. |
| Bubble Tea v2 (Go), Textual (Python), Ink (React, runs Claude Code) | Elm-style update loop, CSS-like styling, `textual serve` for the web. Halftone already builds to wasm for the page. |

## Re-theming other apps across their updates

- Halftone sits between the app and the terminal (a PTY), so it reads only what the app prints. No app internals, nothing to break on update.
- 16-colour output maps exactly to theme slots. 24-bit output is mapped by lightness and hue (above).
- It answers OSC 11 with the theme background, so apps that ask (Claude Code's `auto` theme) choose a matching light or dark scheme.
- Claude Code's default themes are hard-coded 24-bit colours; its `dark-ansi` theme uses only the 16 slots and so re-themes exactly ([theme options](https://blog.vincentqiao.com/en/posts/claude-code-theme/), [issue 39369](https://github.com/anthropics/claude-code/issues/39369)).

## Footage

- Roadster: unveiled mid-October 2026 at a SpaceX site in Texas; Musk: "Flying cars are real" ([CBS](https://www.cbsnews.com/news/tesla-roadster-debut-elon-musk-flying-cars-are-real/)). Default config plays CNET's recap from 1:58 (`media ... from 1:58`); copyrighted, so local playback only, not on the page.
- SpaceX: no catch happened in the last week. Flight 15 (booster and ship catch) is set for no earlier than Nov 8 ([technology.org](https://www.technology.org/2026/10/23/spacex-starship-flight-15-ship-booster-tower-catch-november/)). The Catch entry plays AP's Flight 5 booster catch until then.

## Round 3 (2026-10-30, from your notes)

- Inbox is Gmail and Discord only, top right; X is its own feed card, bottom right. Instagram is gone.
- `connect gmail|discord|x <token or $ENV> [id]` lines poll the real APIs through curl (token on stdin, never in `ps`); errors show on the row ("token refused · check it in the desk file", tested against Gmail with a bogus token). Enter opens the site.
- Now playing is a one-line strip (title, artist, time, rail); no source labels. Enter or a click opens the full player.
- Default desk is one Home tab. Name your own from the palette: type a name, pick "New tab “…”" (opens the editor) or "Rename this tab". First launch with no desk file runs `halftone setup`: Claude Code interviews you and writes the file.
- Watchlist and stock charts are solid green when up, red when down.
- Toasts start under the top row so they never cover the inbox.

## X sign-in (2026-10-30)

- `halftone connect x` signs in through the browser with OAuth 2.0 + PKCE (Native App, callback `http://127.0.0.1:8723/callback`, scopes tweet.read users.read offline.access), saves tokens to `x.token` (mode 600), refreshes them, and adds `connect x @signin`. Tested to the browser hand-off and a cancelled callback; not yet against a real X app.
- X has no free API tier now: reads are pay-per-use, about $0.005 per post, so a 10-post refresh every 5 minutes is roughly $0.60 an hour while the desk is open.
- Default media swapped to public-domain clips: Starman (Falcon Heavy with the Roadster, NASA) and Falcon 9.

## Wishlist

**Data, the X angle first**
- X: home timeline, trending, a post's replies, compose from the palette (API key in config).
- Live quotes (a free feed, user's key), alerts as toasts.
- Stripe/RevenueCat revenue, GitHub PRs and CI, Linear issues.
- Real agent status from Claude Code and Codex transcripts and hooks, not the sample list.

**Terminal**
- Mouse: click cards, scroll terminals, drag split borders.
- Scrollback search and copy mode in terminal cards.
- Sixel and iTerm2 image protocols beside kitty; sextant and octant cells for terminals with neither.
- Per-app retheme overrides (`retheme lazygit 0.3`).
- Session restore: terminals come back where they were.

**Media**
- Spotify/Apple Music now-playing as a source.
- Synced lyrics from LRCLIB when there is no .lrc.
- Queue and shuffle per media tab.

**Making it yours**
- Widget mods as plain scripts: any command that prints lines becomes a card.
- Theme import from Ghostty, iTerm2 and base16 files.
- Shareable desk files (`halftone config import <url>`).
- Agent-facing: `halftone tab add`, `halftone card add`, JSON status out for scripts.

**Voice**
- Push-to-talk on key hold where the terminal reports key release (kitty keyboard protocol).
- Spoken commands for the desk itself ("open Tesla", "next tab").
