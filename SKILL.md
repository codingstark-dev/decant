---
name: decant
description: >
  decant is a CLI tool that mirrors websites and extracts machine-readable design systems
  for AI agents. Use decant to clone/mirror a website's HTML, CSS, JS, fonts, images,
  and screenshots into a local directory, extract design tokens (colors, typography,
  spacing, breakpoints) into design-tokens.json, generate a site manifest and context.md
  for LLM consumption, serve a captured site locally for preview, and re-extract design
  tokens from an existing capture directory.
  Use when asked to "clone this website", "mirror this site", "capture a website",
  "extract design tokens", "get the design system from", "download this site for AI",
  "scrape and preserve a website", "serve a local copy", or "get the CSS/fonts from".
trigger_phrases:
  - "clone this website"
  - "mirror this site"
  - "capture a website"
  - "extract design tokens"
  - "get the design system from"
  - "download this site for AI"
  - "scrape and preserve a website"
  - "serve a local copy"
  - "get the CSS/fonts from"
  - "website design tokens"
  - "mirror and serve"
  - "capture the UI of"
allowed-tools: Bash(decant:*) Bash(cargo:*) Read Write
---

# decant — AI Agent Skill

## Overview

`decant` mirrors a website's HTML/CSS/JS/assets and extracts a machine-readable design
system so AI agents can faithfully reproduce the UI. The output is a local directory
containing all page assets plus three structured AI-friendly files:

| File | Contents |
|------|----------|
| `design-tokens.json` | Colors, typography, spacing, breakpoints, shadows |
| `manifest.json` | Page tree, asset catalog, component regions |
| `context.md` | Human+LLM readable site summary |
| `repair-hints.json` | Clone health, failed asset categories, and AI repair actions |

`decant verify` compares a live URL against a served local capture and writes `verify-report.json` plus live/local PNG screenshots for AI review.

## Installation

```bash
# Via cargo (recommended for developers)
cargo install decant --features render

# Via npm
npm install -g decant-cli

# Via curl (macOS / Linux)
curl -fsSL https://raw.githubusercontent.com/codingstark-dev/decant/main/install.sh | sh
```

## Quick Reference

```
decant clone <URL> [OPTIONS]    # Mirror a website
decant serve <DIR> [OPTIONS]    # Serve a captured site locally
decant verify <LIVE> <LOCAL>    # Compare live/local screenshots
decant tokens <DIR> [OPTIONS]   # Re-extract design tokens
```

## Core Workflows

### 1. Clone a Website (Static — No Browser)

The fastest option. No browser dependency required.

```bash
decant clone https://example.com --output ./example
```

**Output directory will contain:**
```
example/
  index.html
  assets/
    style.css
    app.js
    fonts/
  design-tokens.json
  manifest.json
  context.md
```

---

### 2. Clone with Headless Chrome (Full SPA Support)

For Single Page Applications (React, Vue, Next.js, etc.) that require JavaScript execution:

```bash
decant clone https://example.com --render chrome --output ./example
```

Chrome runtime capture is enabled automatically in `auto` mode. It captures browser-observed static resources such as script-inserted modules, stylesheets, fonts, images, media, manifests, and `.mjs` chunks:

```bash
decant clone https://example.com \
  --render chrome \
  --runtime-capture on \
  --output ./example
```

Runtime capture does not make live backend state, authenticated sessions, WebSocket/SSE streams, protected media, checkout flows, or server-personalized APIs fully offline-cloneable.

After every clone, inspect `repair-hints.json`. If `status` is `needs_repair`, use its categorized issues before judging the capture:

```bash
jq . ./example/repair-hints.json
```

For visual work, serve the capture and run native verification at the same viewport:

```bash
decant serve ./example --port 8080 --noscript
decant verify https://example.com http://127.0.0.1:8080 \
  --viewports desktop \
  --output ./example/verify-report.json \
  --screenshots-dir ./example/verify-screenshots
```

If `verify-report.json` reports `needs_repair`, compare the generated live/local PNGs, repair according to `repair-hints.json`, then rerun `decant clone` and `decant verify`.

---

### 3. Clone with Lightpanda (Lightweight Browser)

Fast headless browser alternative. Smaller memory footprint than Chrome:

```bash
decant clone https://example.com --render lightpanda --output ./example
```

---

### 4. Deep Crawl (Multiple Pages)

Use `--depth` to follow links beyond the seed URL:

```bash
# Crawl up to 2 levels deep
decant clone https://example.com --depth 2 --output ./example

# Single page only (default, depth=0)
decant clone https://example.com --depth 0 --output ./example
```

---

### 5. Clone with Authentication (Cookies)

Pass session cookies to clone authenticated pages:

```bash
# Inline cookie string
decant clone https://example.com \
  --cookies "session=abc123; token=xyz789" \
  --output ./example

# From a Netscape cookie jar file (exported from browser DevTools)
decant clone https://example.com \
  --cookie-file ~/Downloads/cookies.txt \
  --output ./example
```

---

### 6. Clone with Custom Headers

Pass extra HTTP headers (useful for API keys, auth tokens, etc.):

```bash
decant clone https://example.com \
  --header "Authorization: Bearer mytoken" \
  --header "X-API-Key: mykey" \
  --output ./example
```

---

### 7. Control Crawl Scope

By default, decant stays within the seed's origin (same-origin). You can allow additional domains (e.g. CDNs):

```bash
# Allow additional domains for assets (CDNs, font hosts)
decant clone https://example.com \
  --allow-domains fonts.googleapis.com,cdn.example.com \
  --output ./example

# Disable same-origin restriction entirely
decant clone https://example.com \
  --same-origin false \
  --output ./example
```

---

### 8. Control Capture Aspects

Choose exactly which parts of the site to capture:

```bash
# Default: html, css, js, fonts, tokens, context
decant clone https://example.com --output ./example

# HTML and CSS only (skip JS and fonts)
decant clone https://example.com \
  --capture html,css \
  --output ./example

# Everything including screenshots
decant clone https://example.com \
  --capture html,css,js,fonts,images,screenshots,tokens,context \
  --render chrome \
  --output ./example
```

---

### 9. Take Screenshots

Screenshots require a headless browser (`--render chrome`):

```bash
# Screenshots at all viewports (mobile, tablet, desktop)
decant clone https://example.com \
  --render chrome \
  --screenshots mobile,tablet,desktop \
  --output ./example

# Disable screenshots
decant clone https://example.com \
  --render chrome \
  --no-screenshots \
  --output ./example
```

---

### 10. Rate Limiting & Concurrency

Be polite to servers. Tune concurrency and request rate:

```bash
decant clone https://example.com \
  --concurrency 4 \
  --rate-limit 2 \
  --output ./example
```

Defaults: `--concurrency 16`, `--rate-limit 4` (req/s per host).

---

### 11. Ignore robots.txt

By default, decant respects `robots.txt`. To disable:

```bash
decant clone https://example.com \
  --ignore-robots \
  --output ./example
```

---

### 12. Serve a Captured Site Locally

Preview a captured site in a browser without any server setup:

```bash
decant serve ./example
# → http://127.0.0.1:8080
```

**Options:**

```bash
# Custom port and host
decant serve ./example --port 3000 --host 0.0.0.0

# Strip all <script> tags (prevents hydration crashes on SPAs)
decant serve ./example --noscript
```

---

### 13. Re-extract Design Tokens

Re-run token extraction on an existing capture without re-cloning:

```bash
decant tokens ./example

# Force overwrite of existing design-tokens.json
decant tokens ./example --force
```

---

## Output File Reference

### `design-tokens.json`

```json
{
  "schema_version": "1",
  "source": "https://example.com",
  "captured_at": "2026-06-18T00:00:00Z",
  "colors": {
    "swatches": ["#1a1a2e", "#16213e", "#0f3460", "#e94560"]
  },
  "typography": {
    "font_families": ["Inter", "Roboto"],
    "font_sizes": [12, 14, 16, 18, 24, 32, 48],
    "font_weights": [400, 500, 700]
  },
  "spacing": [4, 8, 12, 16, 24, 32, 48, 64],
  "breakpoints": [640, 768, 1024, 1280, 1536]
}
```

### `manifest.json`

```json
{
  "schema_version": "1",
  "captured_at": "2026-06-18T00:00:00Z",
  "pages": [
    {
      "url": "https://example.com/",
      "path": "index.html",
      "title": "Example Domain",
      "regions": []
    }
  ],
  "assets": [...],
  "total_pages": 1,
  "total_assets": 12,
  "total_bytes": 204800
}
```

### `context.md`

A human and LLM readable summary of the captured site, including:
- Site overview and page structure
- Asset inventory
- Design token summary
- Recommended usage guidance for AI agents

---

## Common Patterns for AI Agents

### Pattern 1: Capture → Read design tokens → Reproduce UI
```bash
# Step 1: Clone the site
decant clone https://example.com --output ./example

# Step 2: Read the extracted tokens (AI agent reads this file)
cat ./example/design-tokens.json

# Step 3: Use context.md for a full site summary
cat ./example/context.md
```

### Pattern 2: Clone SPA → Serve locally → Inspect
```bash
# Clone with Chrome (required for JS-heavy SPAs)
decant clone https://example.com --render chrome --runtime-capture auto --output ./example

# Serve locally for browser inspection
decant serve ./example --port 8080
# → Open http://localhost:8080 in browser
```

### Pattern 3: Authenticated Capture
```bash
# Export cookies from your browser (DevTools → Application → Cookies → Export)
decant clone https://dashboard.example.com \
  --cookie-file ./cookies.txt \
  --render chrome \
  --depth 1 \
  --output ./dashboard-capture
```

---

## Troubleshooting

| Problem | Solution |
|---------|----------|
| `Lightpanda` hangs | Use `--render chrome` instead; Lightpanda is in active dev |
| SPA shows blank page | Use `--render chrome` for JS execution |
| Assets missing | Add CDN domains with `--allow-domains cdn.example.com` |
| Rate limited by server | Reduce `--rate-limit 1 --concurrency 2` |
| 403 on pages | Pass cookies with `--cookies` or `--cookie-file` |
| Hydration crash on serve | Use `decant serve --noscript` to strip script tags |
