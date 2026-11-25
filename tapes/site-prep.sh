#!/bin/sh
# Hidden setup for the site recordings: a private desk per recording, 3 throwaway repos, sample todo.
#   tapes/site-prep.sh <name>   -> $TMPDIR/hw/site/<name>/desk.conf
set -e
r=${TMPDIR:-/tmp}/hw/site d=$r/$1
for n in alpha beta gamma; do
  [ -d "$r/repos/$n/.git" ] && continue
  mkdir -p "$r/repos/$n" && git -C "$r/repos/$n" init -q
  echo "# $n" > "$r/repos/$n/README.md"
  git -C "$r/repos/$n" add . && git -C "$r/repos/$n" -c user.name=demo -c user.email=demo@example.com commit -qm init
done
echo wip >> "$r/repos/alpha/README.md"; echo new > "$r/repos/alpha/notes.txt"; echo wip >> "$r/repos/beta/README.md"
rm -rf "$d"; mkdir -p "$d/modules"
cp "$(dirname "$0")/site/$1.conf" "$d/desk.conf"
cp "$(dirname "$0")/site/todo.sh" "$d/modules/todo"; chmod +x "$d/modules/todo"
