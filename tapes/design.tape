# A brand-new desk, every tab, for the design pass.
Output tapes/out/design.mp4
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
Type 'export COLORTERM=truecolor HALFTONE_MUTE=1 HALFTONE_CONFIG=$TMPDIR/hw/fresh/desk.conf ZDOTDIR=$TMPDIR/hw/z; rm -rf $TMPDIR/hw/fresh; ./target/release/halftone config print >/dev/null; clear && ./target/release/halftone'
Enter
Sleep 1s
Show
Sleep 12s
Screenshot tapes/out/design-home.png
Type "]"
Sleep 8s
Screenshot tapes/out/design-dev.png
Type "]"
Sleep 9s
Screenshot tapes/out/design-money.png
Type "]"
Sleep 6s
Screenshot tapes/out/design-read.png
Type "a"
Sleep 1s
Screenshot tapes/out/design-help.png
