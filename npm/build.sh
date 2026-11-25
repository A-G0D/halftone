#!/bin/sh
# Builds halftone for every platform npm ships and stages the packages in npm/out. Publishes nothing.
#   npm/build.sh          build and stage (run on a Mac: the Linux builds link with rust-lld)
#   npm/build.sh --pack   also write npm/out/*.tgz, the exact files `npm publish` would upload
# Layout (as esbuild and Biome do it): halftone-cli holds a small launcher and lists one package per
# platform as an optional dependency; each sets os and cpu, so npm installs only the matching one.
set -e
cd "$(dirname "$0")/.."
pack=$1
version=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
out=npm/out
rm -rf "$out"
mkdir -p "$out"

export CARGO_TARGET_AARCH64_UNKNOWN_LINUX_MUSL_LINKER=rust-lld CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_LINKER=rust-lld
export CARGO_PROFILE_RELEASE_STRIP=symbols
platforms=""
# rust target : npm os : npm cpu. Linux builds are static (musl), so they run on any distro.
for t in aarch64-apple-darwin:darwin:arm64 x86_64-apple-darwin:darwin:x64 \
         aarch64-unknown-linux-musl:linux:arm64 x86_64-unknown-linux-musl:linux:x64; do
  target=${t%%:*}; rest=${t#*:}; os=${rest%%:*}; cpu=${rest#*:}
  name="halftone-cli-$os-$cpu"
  echo "building $name ($target)"
  rustup target add "$target" >/dev/null 2>&1 || true
  cargo build -q --release --locked --target "$target" --bin halftone
  mkdir -p "$out/$name/bin"
  cp "target/$target/release/halftone" "$out/$name/bin/halftone"
  chmod 755 "$out/$name/bin/halftone"
  node -e '
    const [name, version, os, cpu] = process.argv.slice(1);
    const pkg = { name, version, description: `The halftone binary for ${os}-${cpu}. Install halftone-cli instead.`,
      license: "MIT", repository: { type: "git", url: "git+https://github.com/A-G0D/halftone.git" },
      os: [os], cpu: [cpu], files: ["bin/halftone"], preferUnplugged: true };
    require("fs").writeFileSync(`npm/out/${name}/package.json`, JSON.stringify(pkg, null, 2) + "\n");
  ' "$name" "$version" "$os" "$cpu"
  cp LICENSE "$out/$name/"
  platforms="$platforms $name"
done

cp -R npm/halftone-cli "$out/halftone-cli"
cp README.md LICENSE "$out/halftone-cli/"
node -e '
  const fs = require("fs"), [version, ...platforms] = process.argv.slice(1), file = "npm/out/halftone-cli/package.json";
  const pkg = JSON.parse(fs.readFileSync(file, "utf8"));
  pkg.version = version;
  pkg.files.push("README.md", "LICENSE");
  pkg.optionalDependencies = Object.fromEntries(platforms.map((p) => [p, version]));
  fs.writeFileSync(file, JSON.stringify(pkg, null, 2) + "\n");
' "$version" $platforms

if [ "$pack" = "--pack" ]; then
  for p in $platforms halftone-cli; do (cd "$out/$p" && npm pack -q --pack-destination ..); done
fi
echo "staged halftone-cli $version in $out:$platforms"
