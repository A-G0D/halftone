# Site setup: the start card links Stocks in-desk, then the desk shows the tick.
Output tapes/out/site-setup-frames/
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
Type 'sh tapes/site-prep.sh setup; export COLORTERM=truecolor HALFTONE_MUTE=1 HALFTONE_CITY="San Francisco" HALFTONE_ZONES="America/Los_Angeles Europe/London Asia/Tokyo" HALFTONE_REPOS=$TMPDIR/hw/site/repos HALFTONE_CONFIG=$TMPDIR/hw/site/setup/desk.conf ZDOTDIR=$TMPDIR/hw/z; clear && ./target/release/halftone'
Enter
Sleep 1s
Show
Sleep 4s
Screenshot tapes/out/site-setup-a.png
Enter
Sleep 2s
Type@80ms "AAPL NVDA TSLA MSFT"
Sleep 0.8s
Screenshot tapes/out/site-setup-b.png
Enter
Sleep 6s
Screenshot tapes/out/site-setup-c.png
Enter
Sleep 5s
Screenshot tapes/out/site-setup-d.png
Hide
Type "q"
