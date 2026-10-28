"use strict";
// Keep the existing Node check entry point; Rust owns the source scanner.
const { execFileSync } = require("child_process");
const path = require("path");
function findMisplacedTimeouts(dir) {
  const bridge = path.resolve(__dirname, "../../../scripts/lib/devtools_bridge.py");
  return JSON.parse(execFileSync("python3", [bridge, "rpc", "wait-shape"], {
    input: JSON.stringify(dir), encoding: "utf8",
  })).result;
}
module.exports = { findMisplacedTimeouts };
