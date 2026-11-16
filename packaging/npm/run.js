"use strict";
// Runs the native fastxt binary for this platform from vendor/, passing the
// arguments and the exit status through. On macOS and Linux install.js points
// bin/fastxt straight at the binary, so this only runs on Windows, or when npm
// was run with --ignore-scripts.
const { spawnSync } = require("child_process");
const path = require("path");

module.exports = function run() {
  const exe = process.platform === "win32" ? "fastxt.exe" : "fastxt";
  const target = `${process.platform}-${process.arch}`;
  const bin = path.join(__dirname, "vendor", target, exe);
  const result = spawnSync(bin, process.argv.slice(2), { stdio: "inherit" });
  if (result.error) {
    console.error(`fastxt: no prebuilt binary for ${target} (${result.error.message})`);
    process.exit(1);
  }
  if (result.signal) {
    process.kill(process.pid, result.signal);
  }
  process.exit(result.status === null ? 1 : result.status);
};
