# RADR CLI

Command Line Interface (CLI) application to manage Architecture Decision Records (ADRs)

## Overview

- Purpose: Manage Architecture Decision Records (ADRs) from the command line.
- Commands: create, supersede, list, accept, reject, and maintain an `index.md` file.
- Supported config formats: JSON, YAML, or TOML file to choose ADR location and template.

## Installation

### Install (via cargo)

- Requires: Rust toolchain (`cargo` + `rustc`).
- Install: `cargo install radr-cli`
- Run: `radr --help`

### Prebuilt binaries (no Rust required)

- Download from GitHub Releases: navigate to the latest tag and grab the asset matching your OS/arch, e.g.:
  - Linux: `radr-cli-vX.Y.Z-x86_64-unknown-linux-gnu.tar.gz` (also `aarch64`, `armv7`, `i686`). Static builds are available as `*-unknown-linux-musl` for maximum portability.
  - macOS: `radr-cli-vX.Y.Z-{x86_64|aarch64}-apple-darwin.tar.gz`
  - Windows: `radr-cli-vX.Y.Z-{x86_64|i686|aarch64}-pc-windows-msvc.zip`
- Example (Linux x86_64):
  - `tar -xzf radr-cli-vX.Y.Z-x86_64-unknown-linux-gnu.tar.gz`
  - `./radr --help`
- Example (Windows PowerShell):
  - `Expand-Archive radr-cli-vX.Y.Z-x86_64-pc-windows-msvc.zip -DestinationPath .`
  - `./radr.exe --help`

### Install (via source)

- Clone and navigate to the project
- Build from source: `cargo build --release`
- Install locally: `cargo install --path .`
- Run built binary: `target/release/radr --help`

## Usage

- New ADR: `radr new "Adopt PostgreSQL"` (default status: Proposed)
- Supersede ADR: `radr supersede 3 "Move to Managed PostgreSQL"`
- Supersede with force: `radr supersede 3 "Redo Supersede" --force` (allows superseding an ADR even if it is already superseded)
- Reject ADR: `radr reject 3` or `radr reject "Adopt PostgreSQL"`
- List + regenerate index: `radr list` or `radr index`
- Use config: `radr --config radr.toml list` or `RADR_CONFIG=radr.yaml radr list`

## Index

- Written to `<adr_dir>/index.md`.
- Lists all ADRs (active and superseded) with number, title, status, and date.

## Config

- Search order: `--config` path → `RADR_CONFIG` env → local files `radr.toml|yaml|yml|json` or `.radrrc.*`.
- Fields:
  - `adr_dir` (string): Directory where ADRs live. Default: `docs/adr`.
  - `index_name` (string): Name of the index file. Default: `index.md`.
  - `template` (string): Optional path to a custom template.

### Examples

- TOML (`radr.toml`)

```
adr_dir = "docs/adr"
index_name = "index.md"
template = "adr_template.md"
```

- YAML (`radr.yaml`)

```
adr_dir: docs/adr
index_name: index.md
template: adr_template.md
```

- JSON (`radr.json`)

```
{
  "adr_dir": "docs/adr",
  "index_name": "index.md",
  "template": "adr_template.md"
}
```

## Template

- If `template` is set, these placeholders are interpolated:
  - `{{NUMBER}}`, `{{TITLE}}`, `{{DATE}}`, `{{STATUS}}`, `{{SUPERSEDES}}` (empty if none)

## ADR Format

- Default file format created:

```
# ADR 0001: Example Title

Date: 2025-01-01
Status: Accepted
Supersedes: 0003

## Context

## Decision

## Consequences
```

- On supersede, the older ADR is updated with:
  - `Status: Superseded by 000X`
  - `Superseded-by: 000X`

## Notes

- Filenames are `NNNN-title-slug.md` with zero-padded numbering.
- `radr list` regenerates the index and prints a terse table to stdout.
- Works on Windows, macOS, and Linux paths.
