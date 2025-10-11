**RADR CLI**

- Purpose: Manage Architecture Decision Records (ADRs) from the command line.
- Commands: create, supersede, list, and maintain an `index.md`.
- Config: JSON, YAML, or TOML file to choose ADR location and template.

**Install**

- Requires: Rust toolchain (cargo + rustc).
- Build: `cargo build --release`
- Run: `target/release/radr --help`

**Usage**

- New ADR: `radr new "Adopt PostgreSQL"` (default status: Accepted)
- New ADR with status: `radr new "Switch CI" --status Proposed`
- Supersede ADR: `radr supersede 3 "Move to Managed PostgreSQL"`
- Reject ADR: `radr reject 3` or `radr reject "Adopt PostgreSQL"`
- List + regenerate index: `radr list` or `radr index`
- Use config: `radr --config radr.toml list` or `RADR_CONFIG=radr.yaml radr list`

**Index**

- Written to `<adr_dir>/index.md`.
- Lists all ADRs (active and superseded) with number, title, status, and date.

**Config**

- Search order: `--config` path → `RADR_CONFIG` env → local files `radr.toml|yaml|yml|json` or `.radrrc.*`.
- Fields:
  - `adr_dir` (string): Directory where ADRs live. Default: `docs/adr`.
  - `index_name` (string): Name of the index file. Default: `index.md`.
  - `template` (string): Optional path to a custom template.

Examples:

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

**Template**

- If `template` is set, these placeholders are interpolated:
  - `{{NUMBER}}`, `{{TITLE}}`, `{{DATE}}`, `{{STATUS}}`, `{{SUPERSEDES}}` (empty if none)

**ADR Format**

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

**Notes**

- Filenames are `NNNN-title-slug.md` with zero-padded numbering.
- `radr list` regenerates the index and prints a terse table to stdout.
- Works on Windows, macOS, and Linux paths.
