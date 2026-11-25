# btop, gitui, Textual and glow side by side inside halftone, then the theme changes under all four.
Output tapes/out/stacks.mp4
Output tapes/out/stacks-frames/
Set Shell zsh
Set FontFamily "JetBrains Mono"
Set FontSize 16
Set Width 1920
Set Height 1240
Set Padding 0
Set Framerate 30
Set TypingSpeed 0
Set CursorBlink false
Hide
Type 'export COLORTERM=truecolor HALFTONE_DEMO=1 HALFTONE_MUTE=1 HALFTONE_CONFIG=$PWD/tapes/stacks.conf HALFTONE_TERM_CWD=$PWD ZDOTDIR=$TMPDIR/hw/z; clear && ./target/release/halftone'
Enter
Sleep 1s
Show
Sleep 9s
Screenshot tapes/out/stacks-x.png
Type "t"
Sleep 3s
Screenshot tapes/out/stacks-paper.png
Sleep 1s
Hide
Type "q"
