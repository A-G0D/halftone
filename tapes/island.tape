# Dynamic-Island pill: compact, expanded, rotating Live Activities. Big Buck Bunny (CC BY 3.0, Blender Foundation).
# Regenerate: vhs tapes/island.tape  (from the halftone directory)
Output tapes/out/island.mp4
Require cargo
Set Shell zsh
Set FontFamily "JetBrains Mono"
Set Theme { "background": "#0b0c0e", "foreground": "#e6e7ea", "black": "#0b0c0e", "red": "#ff7b72", "green": "#7ee787", "yellow": "#e3b341", "blue": "#79c0ff", "magenta": "#d2a8ff", "cyan": "#76e3ea", "white": "#e6e7ea", "brightBlack": "#5a5e65", "brightRed": "#ffa198", "brightGreen": "#56d364", "brightYellow": "#f0c674", "brightBlue": "#a5d6ff", "brightMagenta": "#e2c5ff", "brightCyan": "#b3f0ff", "brightWhite": "#ffffff", "cursor": "#e6e7ea", "selection": "#2a2d33" }
Set FontSize 28
Set Width 1920
Set Height 1240
Set Padding 0
Set Framerate 30
Set TypingSpeed 0
Set CursorBlink false

Hide
Type "export COLORTERM=truecolor; cargo build -q --release --example island && clear && HALFTONE_MUTE=1 HALFTONE_SCRIPT=1 ./target/release/examples/island $HOME/.cache/halftone/media/sintel.m4a"
Enter
Sleep 1.5s
Sleep 0.3s
Show
Sleep 6s
Screenshot tapes/out/island-a.png
Sleep 11.5s
Hide
Type "q"
