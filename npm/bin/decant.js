#!/usr/bin/env node
/**
 * decant CLI shim — invoked when user runs `decant` after `npm install -g decant`
 * Reads the resolved binary path written by install.js and executes it.
 */

"use strict";

const { spawnSync } = require("child_process");
const fs   = require("fs");
const path = require("path");

const manifestPath = path.join(__dirname, ".bin", "binary-path.json");

if (!fs.existsSync(manifestPath)) {
  console.error(
    "decant: binary not found — try reinstalling with `npm install -g decant`"
  );
  process.exit(1);
}

const { path: bin } = JSON.parse(fs.readFileSync(manifestPath, "utf8"));

const result = spawnSync(bin, process.argv.slice(2), { stdio: "inherit" });
process.exit(result.status ?? 1);
