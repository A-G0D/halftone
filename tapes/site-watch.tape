# Site watch: the media strip opens into the player, the Moon speech as lyrics beside launch video.
Output tapes/out/site-watch-frames/
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
Type 'sh tapes/site-prep.sh watch; export COLORTERM=truecolor HALFTONE_DEMO=1 HALFTONE_MUTE=1 HALFTONE_CITY="San Francisco" HALFTONE_ZONES="America/Los_Angeles Europe/London Asia/Tokyo" HALFTONE_REPOS=$TMPDIR/hw/site/repos HALFTONE_CONFIG=$TMPDIR/hw/site/watch/desk.conf ZDOTDIR=$TMPDIR/hw/z; clear && ./target/release/halftone'
Enter
Sleep 1s
Show
Sleep 4s
Tab
Sleep 0.5s
Enter
Sleep 12s
Screenshot tapes/out/site-watch.png
Hide
Type "q"
