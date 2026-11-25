# The desk end to end: home, a stock opened and its ranges, the now-playing strip opened into
# video and lyrics, a custom tab named from the palette and given a card, then the theme changes.
Output tapes/out/hero.mp4
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
Type 'cp $TMPDIR/hw/hero.conf $TMPDIR/hw/hero-run.conf; export COLORTERM=truecolor HALFTONE_DEMO=1 HALFTONE_MUTE=1 HALFTONE_CONFIG=$TMPDIR/hw/hero-run.conf ZDOTDIR=$TMPDIR/hw/z; clear && ./target/release/halftone'
Enter
Sleep 0.4s
Show
Sleep 9s
Tab
Sleep 0.3s
Tab
Sleep 0.3s
Tab
Sleep 0.6s
Enter
Sleep 1.5s
Right
Sleep 0.8s
Right
Sleep 0.8s
Right
Sleep 1.2s
Escape
Sleep 1s
Shift+Tab
Sleep 0.2s
Shift+Tab
Sleep 0.2s
Shift+Tab
Sleep 0.6s
Enter
Sleep 0.6s
Type "n"
Sleep 10s
Escape
Sleep 1.2s
Ctrl+K
Sleep 0.4s
Type@80ms "Trading"
Sleep 0.8s
Enter
Sleep 1.2s
Type "a"
Sleep 0.6s
Down
Sleep 0.25s
Down
Sleep 0.25s
Down
Sleep 0.5s
Enter
Sleep 1s
Type "e"
Sleep 2s
Type "1"
Sleep 1.5s
Type "t"
Sleep 2.2s
Type "t"
Sleep 3s
Hide
Type "q"
