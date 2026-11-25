#!/bin/sh
# Full bench: fetch images, time halftone, score against Pillow/numpy. Writes bench/results.txt.
set -e
cd "$(dirname "$0")/.."
bench/fetch.sh >/dev/null
{ echo "# $(date +%F) $(uname -sm) $(sysctl -n machdep.cpu.brand_string 2>/dev/null)"
  cargo bench --locked --bench image 2>/dev/null | grep -v '^$'
  python3 -P bench/compare.py
} | tee bench/results.txt
