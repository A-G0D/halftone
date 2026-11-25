# The hero: a dithered planet, lit and re-themed live.
# Regenerate: vhs tapes/demo.tape  (from the halftone directory)
Output tapes/out/demo.mp4
Output tapes/out/demo.webm
Require cargo
Set Shell zsh
Set FontFamily "JetBrains Mono"
Set Theme { "background": "#0b0c0e", "foreground": "#e6e7ea", "black": "#0b0c0e", "red": "#ff7b72", "green": "#7ee787", "yellow": "#e3b341", "blue": "#79c0ff", "magenta": "#d2a8ff", "cyan": "#76e3ea", "white": "#e6e7ea", "brightBlack": "#5a5e65", "brightRed": "#ffa198", "brightGreen": "#56d364", "brightYellow": "#f0c674", "brightBlue": "#a5d6ff", "brightMagenta": "#e2c5ff", "brightCyan": "#b3f0ff", "brightWhite": "#ffffff", "cursor": "#e6e7ea", "selection": "#2a2d33" }
Set FontSize 30
Set Width 2560
Set Height 1520
Set Padding 0
Set Framerate 30
Set TypingSpeed 0
Set CursorBlink false

Hide
Type "export COLORTERM=truecolor; cargo build -q --release --example demo && clear && ./target/release/examples/demo"
Enter
Wait+Screen@10m /t theme/
Show
Sleep 3s
Screenshot tapes/out/demo-a.png
Sleep 5s
Screenshot tapes/out/demo-b.png
Sleep 8s
Hide
Type "q"
