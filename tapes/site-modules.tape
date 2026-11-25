# Site modules: a desk made only of module cards; tab through them.
Output tapes/out/site-modules-frames/
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
Type 'sh tapes/site-prep.sh modules; export COLORTERM=truecolor HALFTONE_DEMO=1 HALFTONE_MUTE=1 HALFTONE_CITY="San Francisco" HALFTONE_ZONES="America/Los_Angeles Europe/London Asia/Tokyo" HALFTONE_REPOS=$TMPDIR/hw/site/repos HALFTONE_CONFIG=$TMPDIR/hw/site/modules/desk.conf ZDOTDIR=$TMPDIR/hw/z; clear && ./target/release/halftone'
Enter
Sleep 1s
Show
Sleep 9s
Screenshot tapes/out/site-modules-a.png
Tab
Sleep 1s
Tab
Sleep 1s
Tab
Sleep 1s
Tab
Sleep 1s
Tab
Sleep 1s
Tab
Sleep 1s
Tab
Sleep 1.5s
Screenshot tapes/out/site-modules-b.png
Hide
Type "q"
