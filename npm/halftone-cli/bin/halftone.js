#!/usr/bin/env node
// Runs the prebuilt halftone binary for this machine. npm installs only the platform package
// whose os and cpu match (see optionalDependencies), so there is nothing to download here.
"use strict";
const { spawn } = require("node:child_process");

const pkg = `halftone-cli-${process.platform}-${process.arch}`;
let bin;
try {
  bin = require.resolve(`${pkg}/bin/halftone`);
} catch {
  console.error(
    `halftone: no prebuilt binary for ${process.platform}-${process.arch} (looked for ${pkg}).\n` +
      "Prebuilt: macOS and Linux, arm64 and x64. If you installed with --no-optional or\n" +
      "--omit=optional, reinstall without it. Or build from source: https://github.com/A-G0D/halftone",
  );
  process.exit(1);
}

const child = spawn(bin, process.argv.slice(2), { stdio: "inherit", env: { ...process.env, HALFTONE_NPM: "1" } });
// Pass signals on, so killing this launcher never leaves halftone running with the terminal.
for (const sig of ["SIGINT", "SIGTERM", "SIGHUP", "SIGQUIT"]) process.on(sig, () => child.kill(sig));
child.on("error", (e) => {
  console.error(`halftone: ${e.message}`);
  process.exit(1);
});
child.on("exit", (code, sig) => {
  if (sig) {
    process.removeAllListeners(sig);
    process.kill(process.pid, sig);
  } else process.exit(code ?? 1);
});
