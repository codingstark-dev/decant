#!/usr/bin/env python3
"""
decant Benchmark Script
=======================
Compares three crawl modes for the target URL:

  Mode 1 — Static      : Pure HTTP fetch, no browser (default)
  Mode 2 — Lightpanda  : CDP-based lightweight JS renderer (~18 MB RAM)
  Mode 3 — Chrome      : Full headless Chrome JS renderer (~1 GB RAM)

For a fair head-to-head, Lightpanda and Chrome use --depth 0 (single page)
since Lightpanda's per-page timeout (15 s) would inflate multi-page totals.
Static mode also runs at depth 0 so all three are directly comparable.

Usage:
  ./benchmark.py                        # benchmarks https://multigres.com
  ./benchmark.py https://example.com   # custom URL

Requirements:
  • ./target/release/decant built with:  cargo build --release --features render
  • lightpanda on PATH or at ~/bin/lightpanda
  • Google Chrome / Chromium on PATH or /Applications/Google Chrome.app
"""

import os
import sys
import time
import subprocess
import shutil

# ── Helpers ────────────────────────────────────────────────────────────────────

def get_child_pids(pid):
    """Return all descendant PIDs of `pid` (recursive)."""
    try:
        out = subprocess.check_output(
            ["pgrep", "-P", str(pid)], stderr=subprocess.DEVNULL
        ).decode()
        children = [int(p) for p in out.split() if p.isdigit()]
        for child in list(children):
            children.extend(get_child_pids(child))
        return children
    except subprocess.CalledProcessError:
        return []


def rss_kb(pid):
    """RSS in KB for a single PID (0 if process already gone)."""
    try:
        out = subprocess.check_output(
            ["ps", "-o", "rss=", "-p", str(pid)], stderr=subprocess.DEVNULL
        ).decode().strip()
        return int(out) if out.isdigit() else 0
    except subprocess.CalledProcessError:
        return 0


def tree_rss_mb(pid):
    """Total RSS (MB) of a process tree rooted at `pid`."""
    pids = [pid] + get_child_pids(pid)
    return sum(rss_kb(p) for p in pids) / 1024.0


def check_binary(name):
    """Return True if `name` is on PATH or at common install paths."""
    if shutil.which(name):
        return True
    extra = {
        "google-chrome": [
            "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome"
        ],
        "chromium": ["/usr/bin/chromium-browser"],
        "lightpanda": [
            os.path.expanduser("~/bin/lightpanda"),
            "/usr/local/bin/lightpanda",
        ],
    }
    for path in extra.get(name, []):
        if os.path.isfile(path):
            return True
    return False


# ── Core runner ────────────────────────────────────────────────────────────────

def run_bench(label, cmd, timeout_secs=120):
    """
    Run *cmd* in a subprocess, sampling RSS every 50 ms until it exits
    or *timeout_secs* elapses.

    Returns dict {time, peak_ram} on success, None on failure/timeout.
    """
    print(f"\n  [{label}] running: {' '.join(cmd)}")
    proc = subprocess.Popen(cmd, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)

    start = time.time()
    deadline = start + timeout_secs
    peak_ram = 0.0

    while True:
        rc = proc.poll()
        if rc is not None:
            break
        if time.time() > deadline:
            print(f"  [{label}] TIMED OUT after {timeout_secs}s — killing")
            proc.kill()
            proc.wait()
            return None
        try:
            ram = tree_rss_mb(proc.pid)
            if ram > peak_ram:
                peak_ram = ram
        except Exception:
            pass
        time.sleep(0.05)

    elapsed = time.time() - start

    if proc.returncode != 0:
        print(f"  [{label}] FAILED (exit code {proc.returncode})")
        return None

    print(f"  [{label}] done in {elapsed:.1f}s  peak RAM {peak_ram:.0f} MB")
    return {"time": elapsed, "peak_ram": peak_ram}


# ── Main ───────────────────────────────────────────────────────────────────────

def main():
    url    = sys.argv[1] if len(sys.argv) > 1 else "https://multigres.com"
    binary = "./target/release/decant"

    if not os.path.isfile(binary):
        sys.exit(
            "ERROR: binary not found.\n"
            "Run: cargo build --release --features render"
        )

    has_chrome     = check_binary("google-chrome") or check_binary("chromium")
    has_lightpanda = check_binary("lightpanda")

    print(f"\ndecant benchmark — target: {url}")
    print(f"  Chrome available    : {'yes' if has_chrome else 'NO — Chrome mode skipped'}")
    print(f"  Lightpanda available: {'yes' if has_lightpanda else 'NO — Lightpanda mode skipped'}")
    print("=" * 60)

    base_flags = ["--tui", "false", "--depth", "0", "--concurrency", "4"]
    results = {}

    # ── Mode 1: Static (no browser, depth=0) ─────────────────────────────────
    out = "target/bench_static"
    shutil.rmtree(out, ignore_errors=True)
    results["Static (no browser)"] = run_bench(
        "Static",
        [binary, "clone", url, "--output", out] + base_flags,
        timeout_secs=60,
    )

    # ── Mode 2: Lightpanda (depth=0) ──────────────────────────────────────────
    if has_lightpanda:
        out = "target/bench_lp"
        shutil.rmtree(out, ignore_errors=True)
        # decant has a 15-second per-page hard timeout; depth=0 → 1 page max
        results["Lightpanda (JS)"] = run_bench(
            "Lightpanda",
            [binary, "clone", url, "--output", out,
             "--render", "lightpanda"] + base_flags,
            timeout_secs=60,
        )
    else:
        results["Lightpanda (JS)"] = None
        print("\n  [Lightpanda] SKIPPED — binary not on PATH")
        print("  Install: https://github.com/nicowillis/lightpanda/releases")

    # ── Mode 3: Chrome (depth=0) ──────────────────────────────────────────────
    if has_chrome:
        out = "target/bench_chrome"
        shutil.rmtree(out, ignore_errors=True)
        results["Chrome (headless)"] = run_bench(
            "Chrome",
            [binary, "clone", url, "--output", out,
             "--render", "chrome"] + base_flags,
            timeout_secs=120,
        )
    else:
        results["Chrome (headless)"] = None
        print("\n  [Chrome] SKIPPED — Chrome not found")

    # ── Results table ─────────────────────────────────────────────────────────
    print()
    print("=" * 60)
    print(" RESULTS  (depth=0, single-page, same URL)")
    print("=" * 60)
    print(f"{'Mode':<24} {'Time (s)':>10} {'Peak RAM (MB)':>14}")
    print("-" * 60)

    static_res = results.get("Static (no browser)")
    for mode, res in results.items():
        if res is None:
            print(f"{mode:<24} {'FAILED / N/A':>10}")
        else:
            speedup = ""
            if static_res and mode != "Static (no browser)":
                ratio = res["time"] / static_res["time"]
                speedup = f"  ({ratio:.1f}× slower)" if ratio > 1.1 else "  (same speed)"
            print(f"{mode:<24} {res['time']:>10.1f} {res['peak_ram']:>14.0f}{speedup}")

    print()
    valid = {k: v for k, v in results.items() if v is not None}
    if len(valid) > 1 and static_res:
        max_ram = max(v["peak_ram"] for v in valid.values())
        if static_res["peak_ram"] > 0 and max_ram > static_res["peak_ram"]:
            print(f"→ Static mode uses {max_ram / static_res['peak_ram']:.0f}× less RAM "
                  f"than the heaviest backend")
    print()


if __name__ == "__main__":
    main()
