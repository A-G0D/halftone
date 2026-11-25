# Artemis I launch (NASA, public domain): the HUD shows on input, then fades to clean video.
# Regenerate: vhs tapes/player.tape  (from the halftone directory)
# VHS drops frames on full-screen truecolor video, so this records at 0.1x (HALFTONE_SPEED, sleeps x10,
# HALFTONE_FPS 3 = 30 fps of content) and is sped back up: ffmpeg -vf "setpts=PTS*8.1/<recorded secs>,fps=30"
Output tapes/out/player.mp4
Require cargo
Set Shell zsh
Set FontFamily "JetBrains Mono"
Set Theme { "background": "#0b0c0e", "foreground": "#e6e7ea", "black": "#0b0c0e", "red": "#ff7b72", "green": "#7ee787", "yellow": "#e3b341", "blue": "#79c0ff", "magenta": "#d2a8ff", "cyan": "#76e3ea", "white": "#e6e7ea", "brightBlack": "#5a5e65", "brightRed": "#ffa198", "brightGreen": "#56d364", "brightYellow": "#f0c674", "brightBlue": "#a5d6ff", "brightMagenta": "#e2c5ff", "brightCyan": "#b3f0ff", "brightWhite": "#ffffff", "cursor": "#e6e7ea", "selection": "#2a2d33" }
Set FontSize 16
Set Width 1920
Set Height 1240
Set Padding 0
Set Framerate 30
Set TypingSpeed 0
Set CursorBlink false
Hide
Type "export COLORTERM=truecolor; cargo build -q --release --example player && clear && HALFTONE_MUTE=1 HALFTONE_SPEED=0.1 HALFTONE_FPS=3 ./target/release/examples/player $HOME/.cache/halftone/media/artemis.mp4"
Enter
Wait+Screen@10m /HALF/
Sleep 5s
Show
Sleep 15s
Screenshot tapes/out/player-a.png
Type "+"
Sleep 6s
Right
Sleep 40s
Screenshot tapes/out/player-b.png
Sleep 20s
Hide
Type "q"
