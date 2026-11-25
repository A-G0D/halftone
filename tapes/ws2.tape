Output tapes/out/ws2.mp4
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
Type "export COLORTERM=truecolor HALFTONE_DEMO=1 HALFTONE_MUTE=1 HALFTONE_CONFIG=$TMPDIR/hw/code.conf HALFTONE_TERM_CWD=$TMPDIR/hwrepo; clear && ./target/release/halftone"
Enter
Wait+Screen@20s /Watchlist/
Sleep 1s
Show
Sleep 5.5s
Screenshot tapes/out/ws2-island.png
Type "2"
Sleep 0.15s
Screenshot tapes/out/ws2-trans.png
Sleep 3s
Screenshot tapes/out/ws2-code.png
Type "3"
Sleep 4s
Screenshot tapes/out/ws2-watch.png
Type "m"
Sleep 2s
Screenshot tapes/out/ws2-disc.png
Hide
Type "q"
