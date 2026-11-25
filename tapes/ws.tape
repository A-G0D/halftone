Output tapes/out/ws.mp4
Require cargo
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
Type "export COLORTERM=truecolor HALFTONE_DEMO=1 HALFTONE_MUTE=1 HALFTONE_CONFIG=$TMPDIR/hw/desk.conf; rm -f $TMPDIR/hw/desk.conf; clear && ./target/release/halftone"
Enter
Wait+Screen@20s /Watchlist/
Sleep 1s
Show
Sleep 4s
Screenshot tapes/out/ws-a.png
Sleep 6s
Screenshot tapes/out/ws-b.png
Tab
Tab
Tab
Enter
Sleep 1.2s
Screenshot tapes/out/ws-stock.png
Right
Right
Sleep 0.2s
Screenshot tapes/out/ws-morph.png
Sleep 1s
Escape
Sleep 1s
Type "e"
Sleep 0.5s
Screenshot tapes/out/ws-edit.png
Type "e"
Ctrl+K
Sleep 0.4s
Type "play"
Sleep 0.4s
Screenshot tapes/out/ws-pal.png
Escape
Sleep 2s
Hide
Type "q"
