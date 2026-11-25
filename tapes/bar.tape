# The same music player in a three-row terminal: a now-playing bar.
# Regenerate: vhs tapes/bar.tape  (from the halftone directory)
Output tapes/out/bar.mp4
Output tapes/out/bar.webm
Require cargo
Set Shell zsh
Set FontFamily "JetBrains Mono"
Set Theme { "background": "#0b0c0e", "foreground": "#e6e7ea", "black": "#0b0c0e", "red": "#ff7b72", "green": "#7ee787", "yellow": "#e3b341", "blue": "#79c0ff", "magenta": "#d2a8ff", "cyan": "#76e3ea", "white": "#e6e7ea", "brightBlack": "#5a5e65", "brightRed": "#ffa198", "brightGreen": "#56d364", "brightYellow": "#f0c674", "brightBlue": "#a5d6ff", "brightMagenta": "#e2c5ff", "brightCyan": "#b3f0ff", "brightWhite": "#ffffff", "cursor": "#e6e7ea", "selection": "#2a2d33" }
Set FontSize 34
Set Width 1920
Set Height 150
Set Padding 0
Set Framerate 30
Set TypingSpeed 0
Set CursorBlink false

Hide
Type "export COLORTERM=truecolor; cargo build -q --release --example music && clear && HALFTONE_MUTE=1 ./target/release/examples/music $HOME/.cache/halftone/media/sintel.m4a"
Enter
Wait+Screen@10m /▶/
Right 3
Sleep 1s
Show
Sleep 3s
Screenshot tapes/out/bar-a.png
Space
Sleep 1.5s
Space
Sleep 2s
Type "t"
Sleep 3s
Hide
Type "q"
