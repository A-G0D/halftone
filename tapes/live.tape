# The desk on live data: real agent sessions, Yahoo quotes in a 2x2 watchlist, inbox link-up hints,
# the X card's connect state, a stock's detail, the income screen and the music view's back button.
Output tapes/out/live.mp4
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
Type 'export COLORTERM=truecolor HALFTONE_MUTE=1 HALFTONE_CONFIG=$TMPDIR/hw/demo-desk.conf ZDOTDIR=$TMPDIR/hw/z; clear && ./target/release/halftone'
Enter
Sleep 1s
Show
Sleep 14s
Screenshot tapes/out/live-desk.png
Ctrl+K
Type "nvda"
Sleep 0.6s
Enter
Sleep 2s
Screenshot tapes/out/live-stock.png
Escape
Sleep 1s
Ctrl+K
Type "income"
Sleep 0.6s
Enter
Sleep 2s
Screenshot tapes/out/live-income.png
Escape
Sleep 1s
