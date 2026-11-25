#!/bin/sh
# Smooth recordings: VHS 0.12 reads its own captured frames at ffmpeg's default 25 fps and then
# resamples to `Set Framerate`, which drops and doubles frames (visible judder). Have the tape also
# write `Output tapes/out/<name>-frames/`, then encode those frames here at their real rate.
#   tapes/encode.sh <frames-dir> <out.mp4> [fps=30] [width=1920]
set -e
dir=$1 out=$2 fps=${3:-30} width=${4:-1920}
ffmpeg -v error -y \
  -framerate "$fps" -i "$dir/frame-text-%05d.png" \
  -framerate "$fps" -i "$dir/frame-cursor-%05d.png" \
  -filter_complex "[0][1]overlay,scale=$width:-2:flags=lanczos,format=yuv420p" \
  -r "$fps" -c:v libx264 -preset slow -crf 18 -movflags +faststart "$out"
# A poster from the last frame, for the page's <video poster>.
ffmpeg -v error -y -sseof -0.1 -i "$out" -frames:v 1 -q:v 3 "${out%.mp4}.jpg"
