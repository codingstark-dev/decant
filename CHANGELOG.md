# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.0] - 2026-06-16

### Added
- Feature-gated SPA rendering with dual browser backends (`chrome` and `lightpanda`).
- Automatic multi-viewport screenshot capture (`mobile`, `tablet`, `desktop`) using headless Chrome.
- Custom headers (`--header`) and cookie parsing (`--cookies`, `--cookie-file`) for crawling behind authentication/paywalls.
- Dynamic aspect selection via `--capture` (selective mirroring of HTML, CSS, JS, fonts, images, screenshots, tokens, context).
- Standard Netscape cookie jar format parser and inline cookie parser.
- Local preview static HTTP server (`serve` subcommand) with path decoding.
- Title extraction from mirrored pages.
- Dynamic list of screenshots in the `context.md` LLM primer.
- Workspace-wide Clippy and documentation warnings configuration.
