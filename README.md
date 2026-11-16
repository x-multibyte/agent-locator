# Agent Locator

A structured specification and registry for identifying AI coding agent tools, their standard configuration paths, and their skills directories across a host machine.

## Overview

This directory defines a strict separation between:

1. **Universal Agent Registry (`agent-tools.json`)**: A static, host-independent catalog of all known AI coding agents and their standard directory conventions. No local machine state or presence assumptions are stored here.
2. **Local Scan Results (`agent-tools-scan-result.json`, pending execution)**: Read-only probe output for a specific host, referencing `agent-tools.json` and recording detected installations, symlink targets, skill counts, and unlisted local agent-related directories.

## Upstream Sources

The universal registry merges two upstream agent registries (87 unique agents total, deduplicated by normalized key and display name):

- [xingkongliang/skills-manager](https://github.com/xingkongliang/skills-manager) (`src-tauri/src/core/tool_adapters.rs` — 56 agents)
- [vercel-labs/skills](https://github.com/vercel-labs/skills) (`src/agents.ts` — 79 agents)

All upstream entries are preserved regardless of whether they exist on the local machine.

## File Inventory

| File                                  | Purpose                                                        |
| :------------------------------------ | :------------------------------------------------------------- |
| `README.md`                           | Project overview, design decisions, and file inventory         |
| `agent-tools.json`                    | Universal static registry of 87 known AI coding agents         |
| `agent-tools-schema.json`             | JSON Schema (Draft 2020-12) for `agent-tools.json`             |
| `agent-tools-scan-result-schema.json` | JSON Schema (Draft 2020-12) for `agent-tools-scan-result.json` |
| `known-agents.json`                   | Initial merged registry snapshot (87 agents)                   |
| `known-agents.md`                     | Human-readable Markdown table of all 87 known agents           |
| `known-agents-schema.json`            | JSON Schema (Draft 2020-12) for `known-agents.json`            |

## Schema Design Decisions

- **Separation of Concerns**: Universal definitions (`agent-tools.json`) never contain host-specific flags such as `installed: true/false`. Host-specific findings belong exclusively in `agent-tools-scan-result.json`.
- **Array-Based Provenance and Paths**: `sources`, `detect_paths`, `global_skills_dirs`, and `additional_scan_dirs` are arrays so multi-source provenance and multi-path/fallback conventions (e.g., OpenClaw legacy paths or Codex shared `~/.agents/skills`) are preserved without truncation.
- **Portable Paths vs. Resolved Paths**: Universal registries use `~` placeholders; scan results record both `configured_path` and `resolved_path`, along with `kind` (`dir`, `file`, `symlink`, `missing`) and `symlink_target`.
- **Dates, Times, and Host Formats**: Aligned with [JSON Schema Dates and Times](https://json-schema.org/understanding-json-schema/reference/type#dates-and-times):
  - Timestamps (`updated_at`, `scanned_at`) use `format: "date-time"` (RFC 3339, section 5.6) paired with a regex `pattern` and `examples` so both format-asserting and annotation-only validators enforce valid timestamps.
  - Scan duration (`scan_duration`) uses `format: "duration"` (ISO 8601 ABNF duration, e.g., `PT0.12S`).
  - Hostname (`host.hostname`) uses `format: "hostname"` (RFC 1123).
- **Compact Non-Installed Tracking**: In `agent-tools-scan-result.json`, agents not found on the host are recorded as a `not_installed_keys` string array rather than 87 verbose empty objects.

## Planned Scan Scope (Not Yet Executed)

When triggered, the read-only scan will:

1. Check every `detect_paths` and `global_skills_dirs` entry in `agent-tools.json`, resolving symlinks and counting skills.
2. Inspect top-level `~/.*` and `~/.config/*` directories plus visible agent-related directories under `~/` (repositories, skill libraries, routers, memory services, MCP servers) to populate `unlisted_local_dirs`.
3. Exclude recursive scans of `~/Library/` and deep workspace trees (`~/dev/`, `~/workspace/`, etc.) to avoid noise.
4. Output `agent-tools-scan-result.json` and validate it against `agent-tools-scan-result-schema.json`.
