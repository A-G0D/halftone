# The desk: a media card with tabs, app shortcuts, coding agents, a watchlist, revenue and news on X.
# Night Owl by Broke For Free (CC BY 3.0), NASA's Artemis I launch, JFK at Rice; the rest is sample data.
# Regenerate: vhs tapes/desk.tape  (from the halftone directory)
Output tapes/out/desk.mp4
Require cargo
Set Shell zsh
Set FontFamily "JetBrains Mono"
Set Theme { "background": "#0b0c0e", "foreground": "#e6e7ea", "black": "#0b0c0e", "red": "#ff7b72", "green": "#7ee787", "yellow": "#e3b341", "blue": "#79c0ff", "magenta": "#d2a8ff", "cyan": "#76e3ea", "white": "#e6e7ea", "brightBlack": "#5a5e65", "brightRed": "#ffa198", "brightGreen": "#56d364", "brightYellow": "#f0c674", "brightBlue": "#a5d6ff", "brightMagenta": "#e2c5ff", "brightCyan": "#b3f0ff", "brightWhite": "#ffffff", "cursor": "#e6e7ea", "selection": "#2a2d33" }
Set FontSize 18
Set Width 1920
Set Height 1240
Set Padding 0
Set Framerate 30
Set TypingSpeed 0
Set CursorBlink false
Hide
Type "export COLORTERM=truecolor; cargo build -q --release --example desk && clear && HALFTONE_MUTE=1 ./target/release/examples/desk"
Enter
Wait+Screen@10m /Watchlist/
Sleep 1.5s
Show
Sleep 4s
Screenshot tapes/out/desk-a.png
Sleep 4s
Tab
Sleep 4s
Screenshot tapes/out/desk-v.png
Tab
Sleep 3s
Tab
Sleep 3s
Screenshot tapes/out/desk-b.png
Sleep 2s
Hide
Type "q"
