#!/usr/bin/env node
/**
 * decant — npm package installer
 *
 * Resolves the correct platform-specific binary and creates a shim so that
 * `decant` works from the command line after `npm install -g decant`.
 *
 * Binary resolution order:
 *   1. Platform optional dependency (decant-darwin-arm64, etc.)
 *   2. GitHub Releases download (fallback)
 */

"use strict";

const { execFileSync, spawnSync } = require("child_process");
const fs   = require("fs");
const os   = require("os");
const path = require("path");
const https = require("https");

const VERSION = require("./package.json").version;
const REPO    = "codingstark-dev/decant";

// ── Platform → optional package name ─────────────────────────────────────────
const PLATFORM_PACKAGES = {
  "darwin-arm64":  "decant-darwin-arm64",
  "darwin-x64":    "decant-darwin-x64",
  "linux-x64":     "decant-linux-x64",
  "linux-arm64":   "decant-linux-arm64",
  "win32-x64":     "decant-win32-x64",
};

const PLATFORM_EXTS = {
  "win32": ".exe",
};

function platformKey() {
  return `${os.platform()}-${os.arch()}`;
}

function binaryExt() {
  return PLATFORM_EXTS[os.platform()] || "";
}

/** Try to resolve binary from an installed optional dependency. */
function findFromOptionalDep() {
  const pkg = PLATFORM_PACKAGES[platformKey()];
  if (!pkg) return null;
  try {
    // Require the optional dep to get its root directory
    const depDir = path.dirname(require.resolve(`${pkg}/package.json`));
    const bin    = path.join(depDir, "bin", `decant${binaryExt()}`);
    if (fs.existsSync(bin)) return bin;
  } catch {
    // optional dep not installed — fall through to download
  }
  return null;
}

/** Download the binary from GitHub Releases. */
function downloadBinary(dest) {
  const platform = os.platform();   // darwin | linux | win32
  const arch     = os.arch();       // arm64  | x64

  // Normalise to release asset name convention used in CI
  const archMap  = { arm64: "aarch64", x64: "x86_64" };
  const osMap    = { darwin: "apple-darwin", linux: "unknown-linux-musl", win32: "pc-windows-msvc" };
  const ext      = platform === "win32" ? ".zip" : ".tar.gz";

  const target   = `${archMap[arch] || arch}-${osMap[platform] || platform}`;
  const asset    = `decant-v${VERSION}-${target}${ext}`;
  const url      = `https://github.com/${REPO}/releases/download/v${VERSION}/${asset}`;

  console.log(`  Downloading decant ${VERSION} for ${target}…`);
  console.log(`  ${url}`);

  // We use curl/wget as a simpler cross-platform download approach
  const tmpDir  = fs.mkdtempSync(path.join(os.tmpdir(), "decant-"));
  const archive = path.join(tmpDir, asset);

  const curl = spawnSync("curl", ["-fsSL", "-o", archive, url], { stdio: "inherit" });
  if (curl.status !== 0) {
    throw new Error(`curl failed — check ${url} exists`);
  }

  // Extract
  if (ext === ".tar.gz") {
    spawnSync("tar", ["-xzf", archive, "-C", tmpDir], { stdio: "inherit" });
  }
  // else unzip on Windows (not implemented here — use platform pkg instead)

  const extracted = path.join(tmpDir, `decant${binaryExt()}`);
  fs.mkdirSync(path.dirname(dest), { recursive: true });
  fs.copyFileSync(extracted, dest);
  fs.chmodSync(dest, 0o755);
  fs.rmSync(tmpDir, { recursive: true, force: true });
}

// ── Main ─────────────────────────────────────────────────────────────────────

const binDir  = path.join(__dirname, ".bin");
const binPath = path.join(binDir, `decant${binaryExt()}`);

// 1. Try optional dependency first (fast, no network)
let resolved = findFromOptionalDep();

// 2. Fall back to GitHub download
if (!resolved) {
  try {
    downloadBinary(binPath);
    resolved = binPath;
  } catch (err) {
    console.error(`\n  ✗ Could not install decant binary: ${err.message}`);
    console.error(`  Try: cargo install decant-cli\n`);
    process.exit(1);
  }
}

console.log(`  ✓ decant binary installed: ${resolved}`);

// Write the resolved path to a manifest so bin/decant.js can find it
fs.mkdirSync(binDir, { recursive: true });
fs.writeFileSync(
  path.join(__dirname, ".bin", "binary-path.json"),
  JSON.stringify({ path: resolved }, null, 2)
);
