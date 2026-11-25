#!/bin/sh
# Downloads the three CC0 bench images from Wikimedia Commons into bench/images/ (git-ignored)
# and converts them to BMP with macOS sips (any tool that writes 24-bit BMP or P6 PPM works).
set -e
cd "$(dirname "$0")"
mkdir -p images && cd images
get() { # name, commons file title, width
  [ -f "$1.bmp" ] && return
  curl -sSfL -m 120 -A "halftone-bench/0.1" -o "$1.jpg" "https://commons.wikimedia.org/wiki/Special:FilePath/$2?width=$3"
  sips -s format bmp "$1.jpg" --out "$1.bmp" >/dev/null
  rm "$1.jpg"
}
get urbino   "Landscape_South_of_Urbino,_Taken_from_Parco_della_Resistenza_at_Late_Sunset.jpg" 2048
get met_landscape "Landscape_MET_ap29.35.jpg" 1600
get portrait_painting "Landscape_Painting_01.jpg" 1200
ls -l *.bmp
