# The default desk, the now-playing strip opening the player, and a custom tab made from the palette.
Output tapes/out/home.mp4
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
Type 'cp $TMPDIR/hw/fresh.conf $TMPDIR/hw/home.conf; export COLORTERM=truecolor HALFTONE_DEMO=1 HALFTONE_MUTE=1 HALFTONE_CONFIG=$TMPDIR/hw/home.conf ZDOTDIR=$TMPDIR/hw/z; clear && ./target/release/halftone'
Enter
Sleep 1s
Show
Sleep 12s
Screenshot tapes/out/home-desk.png
Enter
Sleep 2s
Screenshot tapes/out/home-player.png
Escape
Sleep 1s
Ctrl+K
Type "Trading"
Sleep 1s
Screenshot tapes/out/home-palette.png
Enter
Sleep 1.5s
Screenshot tapes/out/home-newtab.png
Sleep 1s
Hide
Type "e"
Sleep 0.5s
Type "q"
