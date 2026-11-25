# One Playback, three layouts (bar, card, panel) built from the same deck parts. Big Buck Bunny (CC BY 3.0, Blender Foundation).
# Regenerate: vhs tapes/compose.tape  (from the halftone directory)
Output tapes/out/compose.mp4
Require cargo
Set Shell zsh
Set FontFamily "JetBrains Mono"
Set Theme { "background": "#0b0c0e", "foreground": "#e6e7ea", "black": "#0b0c0e", "red": "#ff7b72", "green": "#7ee787", "yellow": "#e3b341", "blue": "#79c0ff", "magenta": "#d2a8ff", "cyan": "#76e3ea", "white": "#e6e7ea", "brightBlack": "#5a5e65", "brightRed": "#ffa198", "brightGreen": "#56d364", "brightYellow": "#f0c674", "brightBlue": "#a5d6ff", "brightMagenta": "#e2c5ff", "brightCyan": "#b3f0ff", "brightWhite": "#ffffff", "cursor": "#e6e7ea", "selection": "#2a2d33" }
Set FontSize 24
Set Width 1920
Set Height 1240
Set Padding 0
Set Framerate 30
Set TypingSpeed 0
Set CursorBlink false

Hide
Type "export COLORTERM=truecolor; cargo build -q --release --example compose && clear && HALFTONE_MUTE=1 ./target/release/examples/compose $HOME/.cache/halftone/media/bunny.mkv"
Enter
Wait+Screen@10m /space/
Right 1
Sleep 1s
Show
Sleep 5s
Screenshot tapes/out/compose-a.png
Type "t"
Sleep 4s
Screenshot tapes/out/compose-b.png
Type "t"
Sleep 4s
Hide
Type "q"
