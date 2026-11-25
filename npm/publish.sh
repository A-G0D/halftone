#!/bin/sh
# Publish halftone-cli to npm from this Mac: signs you in if needed, builds all four platforms,
# then publishes the platform packages before the main one.   sh npm/publish.sh
set -e
cd "$(dirname "$0")/.."
npm whoami >/dev/null 2>&1 || npm login --auth-type=web
echo "Publishing as $(npm whoami)"
sh npm/build.sh
for p in halftone-cli-darwin-arm64 halftone-cli-darwin-x64 halftone-cli-linux-arm64 halftone-cli-linux-x64 halftone-cli; do
  npm publish "npm/out/$p" --access public --auth-type=web
done
echo "Done: npm i -g halftone-cli"
