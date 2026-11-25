# Site hero: a Home desk of media, apps, agents, weather, system, clocks and a watch row, then the palette and a toast.
Output tapes/out/site-hero-frames/
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
Type 'sh tapes/site-prep.sh hero; export COLORTERM=truecolor HALFTONE_DEMO=1 HALFTONE_MUTE=1 HALFTONE_CITY="San Francisco" HALFTONE_ZONES="America/Los_Angeles Europe/London Asia/Tokyo" HALFTONE_REPOS=$TMPDIR/hw/site/repos HALFTONE_CONFIG=$TMPDIR/hw/site/hero/desk.conf ZDOTDIR=$TMPDIR/hw/z; clear && ./target/release/halftone'
Enter
Sleep 1s
Show
Sleep 9s
Ctrl+K
Sleep 0.6s
Type "theme"
Sleep 2s
Escape
Sleep 3s
Screenshot tapes/out/site-hero.png
Hide
Type "q"
