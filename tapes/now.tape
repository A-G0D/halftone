# The now-playing strip on the desk and its large view (sample track in demo mode).
Output tapes/out/now.mp4
Set Shell zsh
Set FontFamily "JetBrains Mono"
Set FontSize 18
Set Width 1920
Set Height 1240
Set Padding 0
Set Framerate 30
Set TypingSpeed 0
Set CursorBlink false
Hide
Type 'export COLORTERM=truecolor HALFTONE_DEMO=1 HALFTONE_MUTE=1 HALFTONE_CONFIG=$TMPDIR/hw/demo-desk.conf ZDOTDIR=$TMPDIR/hw/z; clear && ./target/release/halftone'
Enter
Sleep 0.4s
Show
Sleep 4s
Enter
Sleep 4s
Escape
Sleep 1.5s
Hide
Type "q"
