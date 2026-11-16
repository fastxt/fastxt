"use strict";
// On macOS and Linux, point bin/fastxt straight at vendor/<platform>-<arch>/fastxt so running
// the command starts the native binary without Node in between. On Windows, or when npm runs
// with --ignore-scripts, the Node wrapper in bin/ stays and runs the same binary through run.js.
const fs = require("fs");
const path = require("path");

if (process.platform === "win32") {
  process.exit(0);
}

const target = `${process.platform}-${process.arch}`;
const rel = path.join("..", "vendor", target, "fastxt");
const binDir = path.join(__dirname, "bin");
const binary = path.join(binDir, rel);

if (!fs.existsSync(binary)) {
  process.exit(0);
}

try {
  fs.chmodSync(binary, 0o755);
  const dest = path.join(binDir, "fastxt");
  fs.unlinkSync(dest);
  fs.symlinkSync(rel, dest);
} catch {
  // Keep the Node wrapper if the filesystem refuses symlinks.
}
