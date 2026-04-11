# Intl Lens - Roadmap

## Project Overview

**Intl Lens** is an AI-powered internationalization tool providing LSP, CLI, and MCP interfaces for i18n management.

---

## Version History

| Version | Date | Changes |
|---------|------|---------|
| 0.1.4 | 2026-01 | Current stable - LSP features for Zed |
| 0.2.0 | TBD | Add CLI and MCP server |

---

## Implementation Status

### Phase 1: Core Library & Infrastructure

| Feature | Status | Notes |
|---------|--------|-------|
| Project restructure (lib + binaries) | 🟡 In Progress | LSP + CLI + MCP in one crate |
| Public API exports (lib.rs) | 🟡 In Progress | Has compilation errors |
| Configuration module (config.rs) | ✅ Complete | Already existed |
| Translation storage (store.rs) | ✅ Complete | Already existed |
| Key finder (key_finder.rs) | ✅ Complete | Already existed |
| File parser (parser.rs) | ✅ Complete | Already existed |

### Phase 2: LSP Server (main.rs)

| Feature | Status | Notes |
|---------|--------|-------|
| Inline hints | ✅ Complete | Original feature |
| Hover preview | ✅ Complete | Original feature |
| Missing key detection | ✅ Complete | Original feature |
| Incomplete coverage | ✅ Complete | Original feature |
| Autocomplete | ✅ Complete | Original feature |
| Go to definition | ✅ Complete | Original feature |
| Auto reload | ✅ Complete | Original feature |

### Phase 3: CLI Tool (cli.rs)

| Feature | Status | Notes |
|---------|--------|-------|
| Command structure (clap) | ✅ Complete | `audit`, `check`, `fix` commands |
| Audit command | ✅ Complete | Full project audit |
| Check command | ✅ Complete | Check specific files |
| Fix command | 🟡 Stub | Just prints "coming soon" |
| Output: Terminal (colored) | ✅ Complete | With progress bar |
| Output: JSON | ✅ Complete | Machine-readable |
| Output: Markdown | ✅ Complete | Documentation-friendly |
| Missing translation detection | ✅ Complete | In audit module |
| Unused key detection | ✅ Complete | In audit module |
| Placeholder validation | ✅ Complete | In audit module |
| Fix suggestions | ✅ Complete | AI-actionable output |

### Phase 4: MCP Server (mcp.rs)

| Feature | Status | Notes |
|---------|--------|-------|
| MCP protocol implementation | 🔴 Not Started | Just a stub |
| audit_i18n tool | 🔴 Not Started | - |
| get_missing_translations tool | 🔴 Not Started | - |
| suggest_translation_fixes tool | 🔴 Not Started | - |
| validate_placeholders tool | 🔴 Not Started | - |
| Resources (translation files) | 🔴 Not Started | - |

### Phase 5: Code Scanner (scanner.rs)

| Feature | Status | Notes |
|---------|--------|-------|
| Directory scanning | ✅ Complete | Scans .ts, .tsx, .js, .jsx, .vue, .php, .dart |
| Key detection | ✅ Complete | Uses KeyFinder patterns |
| Code snippet extraction | ✅ Complete | Shows line context |
| Skip patterns | ✅ Complete | node_modules, .git, target, etc. |

### Phase 6: Audit Module (audit.rs)

| Feature | Status | Notes |
|---------|--------|-------|
| AuditReport struct | ✅ Complete | Serializes to JSON/Markdown |
| Missing translation detection | ✅ Complete | Checks all keys against all locales |
| Unused key detection | ✅ Complete | Compares keys vs. source code |
| Placeholder validation | ✅ Complete | Detects mismatches |
| Fix suggestions | ✅ Complete | AI-actionable commands |
| Placeholder extraction | ✅ Complete | Handles {{name}}, {name}, %s patterns |

---

## Current Issues

### Compilation Errors (Blocking)

1. **Type inference issues** in `audit.rs`:
   - `contains_key` needs type annotation
   - `get_translation` returns `&str` not `String`

2. **Unused imports** warnings:
   - Multiple files have unused imports

3. **Module structure**:
   - `lib.rs` exports need verification

### Remaining Work

- [x] Fix compilation errors in audit.rs and scanner.rs
- [ ] Complete MCP server implementation
- [ ] Add tests for CLI and audit module
- [ ] Update README with new CLI/MCP usage
- [ ] Publish v0.2.0

---

## Roadmap by Priority

### P0 - Critical (Must Fix)
- [x] Fix compilation errors
- [x] Verify CLI `audit` command works

### P1 - High Priority
- [ ] Complete MCP server tools
- [ ] Add integration tests

### P2 - Medium Priority
- [ ] Add `--fix` command with dry-run
- [ ] Performance optimization for large codebases

### P3 - Nice to Have
- [ ] Git integration (detect new keys from diff)
- [ ] Translation API integration (DeepL, Google)
- [ ] VS Code LSP client support

---

## Dependencies Graph

```
                    ┌─────────────────────────────────────┐
                    │           intl-lens crate            │
                    │        (lib + 3 binaries)            │
                    └─────────────────────────────────────┘
                    ▲           ▲           ▲           ▲
         ┌──────────┴──┐  ┌────┴────┐  ┌───┴────┐  ┌────┴─────┐
         │ main.rs     │  │ cli.rs  │  │ mcp.rs │  │  lib.rs   │
         │ (LSP)      │  │ (CLI)   │  │ (MCP)  │  │ (exports) │
         └─────────────┘  └─────────┘  └────────┘  └──────────┘
                                          │         │
                                          ▼         ▼
                               ┌──────────────┐ ┌────────────┐
                               │ scanner.rs   │ │ audit.rs   │
                               │ (scanning)   │ │ (reporting)│
                               └──────────────┘ └────────────┘
                                          │
                                          ▼
                               ┌─────────────────────┐
                               │       i18n/         │
                               │ store.rs           │
                               │ key_finder.rs      │
                               │ parser.rs          │
                               │ config.rs          │
                               └─────────────────────┘
```

---

## Release Timeline

| Milestone | Target | Features |
|-----------|--------|----------|
| v0.2.0-alpha | 2026-Q2 | CLI audit works, basic MCP |
| v0.2.0-beta | 2026-Q3 | Full MCP, tests |
| v0.2.0-stable | 2026-Q4 | Production-ready |

---

## Contribution Areas

Looking for contributions in:
1. MCP server implementation
2. CLI testing
3. Documentation
4. More framework support

---

*Last Updated: 2026-04-11*
*Version: 0.2.0 WIP*
