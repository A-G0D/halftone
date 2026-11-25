# Voice mod over a plain shell: ctrl+space listens, ctrl+space again sends what whisper heard.
Output tapes/out/voice.mp4
Output tapes/out/voice-frames/
Set Shell zsh
Set FontFamily "JetBrains Mono"
Set FontSize 18
Set Width 1920
Set Height 700
Set Padding 0
Set Framerate 30
Set TypingSpeed 0
Set CursorBlink false
Hide
Type 'export COLORTERM=truecolor HALFTONE_VOICE_FILE=$TMPDIR/hw/voice.aiff HALFTONE_CONFIG=$TMPDIR/hw/voice.conf ZDOTDIR=$TMPDIR/hw/z; clear && ./target/release/halftone wrap zsh'
Enter
Sleep 1s
Type 'clear'
Enter
Sleep 0.6s
Show
Sleep 1s
Ctrl+V
Sleep 3s
Screenshot tapes/out/voice-listen.png
Ctrl+V
Sleep 3s
Screenshot tapes/out/voice-done.png
Sleep 1s
Hide
Ctrl+C
Type "exit"
Enter
