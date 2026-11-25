# Site stock: open a stock from the watch card and change the range.
Output tapes/out/site-stock-frames/
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
Type 'sh tapes/site-prep.sh stock; export COLORTERM=truecolor HALFTONE_DEMO=1 HALFTONE_MUTE=1 HALFTONE_CITY="San Francisco" HALFTONE_ZONES="America/Los_Angeles Europe/London Asia/Tokyo" HALFTONE_REPOS=$TMPDIR/hw/site/repos HALFTONE_CONFIG=$TMPDIR/hw/site/stock/desk.conf ZDOTDIR=$TMPDIR/hw/z; clear && ./target/release/halftone'
Enter
Sleep 1s
Show
Sleep 6s
Enter
Sleep 2.5s
Screenshot tapes/out/site-stock-a.png
Right
Sleep 1.5s
Right
Sleep 1.5s
Right
Sleep 1.5s
Right
Sleep 2s
Screenshot tapes/out/site-stock-b.png
Escape
Sleep 1s
Hide
Type "q"
