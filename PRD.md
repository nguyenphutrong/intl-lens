# Intl Lens - Product Requirements Document

## 1. Project Overview

### Project Name
**Intl Lens** - i18n Intelligence Layer for Codebases

### Project Type
Open-source developer tool: reusable Rust core with LSP, CLI, and MCP surfaces for internationalization (i18n) management.

### Core Feature Summary
Intl Lens gives developers, CI systems, and AI coding agents structured visibility into translation keys, coverage, placeholder safety, and source usage across multiple frameworks and languages.

### Target Users
- **Primary**: Software developers working on multi-language applications
- **Secondary**: DevOps/CI teams requiring i18n validation in build pipelines
- **Tertiary**: AI coding agents (Codex, Claude, Cursor, etc.) that need to understand and manage translation states
- **Future**: QA, product, content, and translator teams using a dashboard or desktop manager

---

## 2. Problem Statement

### Current Pain Points

1. **Invisible Translations**: Developers can't see translation values inline while coding
2. **Missing Key Discovery**: No easy way to know which translation keys are missing across locales
3. **Manual Auditing**: Checking i18n coverage requires manual work or custom scripts
4. **AI Agent Integration**: AI coding agents have no standardized way to query i18n state
5. **Placeholder Mismatches**: Translation placeholders (e.g., `{{name}}` vs `{name}`) often mismatch across locales, causing runtime errors

### Market Gap
No unified tool combines LSP-level editor integration, CI-friendly auditing, and an AI-agent MCP interface for i18n management across multiple frameworks.

---

## 3. Goals & Non-Goals

### Goals

1. **Reusable Core**: Keep scanner, parser, config, store, and audit logic shared across all surfaces
2. **Editor Integration**: Provide real-time i18n hints, autocomplete, and go-to-definition in Zed first, then other LSP-capable editors
3. **CI-Ready Auditing**: Detect missing translations, unused keys, and placeholder mismatches with machine-readable output and meaningful exit codes
4. **AI-Ready Interface**: Enable AI agents to query translation state and receive actionable fix suggestions
5. **Multi-Framework Support**: Support major i18n libraries: react-i18next, vue-i18n, Laravel, Flutter, Angular, etc.

### Non-Goals

1. **Cloud Service**: Intl Lens should not require a hosted backend for core auditing
2. **Unreviewed Translation Writes**: Provider-generated or AI-generated translations must go through review or dry-run workflows before write mode
3. **Editor Dependency**: Zed is the first editor surface, but CLI and MCP must work independently
4. **Team Collaboration Platform**: Comments, assignments, and hosted review workflows are out of scope until the local tooling is stable

---

## 4. Technical Architecture

### 4.1 Module Structure

```
intl-lens (Rust crate)
├── lib.rs              # Public API exports
├── main.rs             # LSP server binary (Zed extension)
├── cli.rs              # CLI tool binary
├── mcp.rs              # MCP server binary (AI agent interface)
├── config.rs           # Configuration loading
├── audit.rs            # i18n auditing logic
├── scanner.rs          # Source code scanning
└── i18n/
    ├── store.rs        # Translation storage
    ├── parser.rs       # File format parsing (JSON, YAML, PHP, ARB)
    └── key_finder.rs   # i18n key pattern matching
```

### 4.2 Binary Targets

| Binary | Purpose | Entry Point |
|--------|---------|-------------|
| `intl-lens` | LSP server with public CLI subcommands | `main.rs` |
| `intl-lens-cli` | Compatibility CLI alias | `cli.rs` |
| `intl-lens-mcp` | AI agent integration | `mcp.rs` |

### 4.3 Key Dependencies

- **tower-lsp**: LSP protocol implementation
- **clap**: CLI argument parsing
- **colored**: Terminal output styling
- **indicatif**: Progress indicators
- **serde**: Serialization (JSON output)
- **dashmap**: Concurrent translation storage
- **walkdir**: Directory traversal
- **globset**: File pattern matching
- **regex**: Key pattern matching

---

## 5. Feature Specifications

### 5.1 LSP Server (Zed Extension)

| Feature | Description | Status |
|---------|-------------|--------|
| Inline Hints | Show translation value next to i18n key | ✅ Implemented |
| Hover Preview | Show all locale translations with jump links | ✅ Implemented |
| Missing Key Detection | Warn on undefined translation keys | ✅ Implemented |
| Incomplete Coverage | Show which locales are missing | ✅ Implemented |
| Autocomplete | Suggest keys with preview | ✅ Implemented |
| Go to Definition | Jump to translation in locale files | ✅ Implemented |
| Auto Reload | Watch translation files for changes | ✅ Implemented |

### 5.2 CLI Tool

#### Commands

```bash
intl-lens audit [OPTIONS]
intl-lens ci [OPTIONS]
intl-lens check <files>...
intl-lens fix [OPTIONS]
```

#### Options

| Flag | Description | Default |
|------|-------------|---------|
| `--workspace, -w` | Project root path | `.` |
| `--format` | Output format: terminal, json, markdown | `terminal` |
| `--output, -o` | Output file path | stdout |
| `--missing-in` | Filter by missing locales | all |
| `--suggest-fixes` | Include AI fix suggestions | false |

#### Output Formats

**Terminal** (colored):
```
╔══════════════════════════════════════╗
║      i18n Audit Report               ║
╚══════════════════════════════════════╝

Summary
  Total Keys:        150
  Missing Translations: 23 ⚠️
  Unused Keys:       5 ⚠️

Missing Translations
  • common.buttons.save
    Source (en): Save
    Missing in: vi, ja
    Used in:
      - src/components/Form.tsx:42
```

**JSON** (machine-readable):
```json
{
  "summary": {
    "total_keys": 150,
    "missing_translations": 23
  },
  "missing": [...]
}
```

**Markdown** (documentation):
```markdown
# i18n Audit Report

## Missing Translations

### `common.buttons.save`
- **Source (en):** Save
- **Missing in:** `vi`, `ja`
```

### 5.3 MCP Server

#### Tools

| Tool | Description | Input |
|------|-------------|-------|
| `audit_i18n` | Full project audit | `{scope, include_suggestions}` |
| `get_missing_translations` | List missing keys | `{locales, include_context}` |
| `suggest_translation_fixes` | Get fix suggestions | `{key, target_locales}` |
| `validate_placeholders` | Check placeholder consistency | `{key}` |

#### Resources

- Translation files (read-only)
- Configuration files
- Audit reports

### 5.4 Auditing Capabilities

| Capability | Description |
|------------|-------------|
| **Missing Translation Detection** | Find keys used in code but missing in some locales |
| **Unused Key Detection** | Find translation keys not referenced in any source file |
| **Placeholder Validation** | Detect mismatched placeholders (e.g., `{{name}}` vs `{name}`) |
| **Coverage Reporting** | Show percentage of translations per locale |
| **Fix Suggestions** | AI-actionable commands to add missing keys |

---

## 6. Supported Frameworks & Formats

### 6.1 i18n Frameworks

| Framework | Patterns |
|-----------|----------|
| react-i18next | `t("key")`, `useTranslation()`, `<Trans i18nKey>` |
| vue-i18n | `$t("key")`, `$tc("key")`, `$te("key")` |
| react-intl | `formatMessage({ id: "key" })` |
| ngx-translate (Angular) | `translateService.instant("key")`, `\| translate` |
| Transloco (Angular) | `translocoService.translate("key")`, `\| transloco` |
| Laravel/PHP | `__("key")`, `trans("key")`, `@lang("key")` |
| Flutter gen_l10n | `AppLocalizations.of(context)!.key` |
| Flutter easy_localization | `'key'.tr()`, `tr('key')`, `context.tr()` |
| Flutter flutter_i18n | `FlutterI18n.translate(context, 'key')`, `I18nText('key')` |
| Flutter GetX | `'key'.tr`, `'key'.trParams({})` |

### 6.2 Supported Languages (Source Files)

- TypeScript / TSX
- JavaScript / JSX
- Vue.js (`.vue`)
- HTML
- Angular templates
- PHP
- Blade (`.blade.php`)
- Dart (Flutter)

### 6.3 Translation File Formats

| Format | Extensions |
|--------|------------|
| JSON | `.json` |
| YAML | `.yaml`, `.yml` |
| PHP | `.php` |
| ARB (Flutter) | `.arb` |

### 6.4 Locale Detection

- Directory per locale: `locales/en/`, `locales/vi/`
- Flat files: `en.json`, `vi.json`
- ARB naming: `app_en.arb`, `app_vi.arb`

---

## 7. Configuration

### 7.1 Configuration File Location

Priority order:
1. `.i18n-ally.json`
2. `i18n-ally.config.json`
3. `.zed/i18n.json`

### 7.2 Configuration Schema

```json
{
  "localePaths": ["locales", "src/i18n"],
  "sourceLocale": "en",
  "keyStyle": "nested" | "flat" | "auto",
  "namespaceEnabled": false,
  "functionPatterns": [
    "t\\s*\\(\\s*[\"']([^\"']+)[\"']"
  ]
}
```

### 7.3 Default Locale Paths

```json
["locales", "i18n", "translations", "public/locales", "src/locales", "src/i18n"]
```

---

## 8. User Flows

### 8.1 Developer Flow (Editor)

1. Open project in Zed or another future LSP client
2. Type `t("common.buttons.save")`
3. See inline hint: "Save"
4. Hover to see all locale translations
5. Missing key shows warning

### 8.2 Developer Flow (CLI)

```bash
# Quick audit
$ intl-lens audit

# Generate report for CI/CD
$ intl-lens audit --format json > i18n-report.json

# Check specific files
$ intl-lens check src/components/*.tsx
```

### 8.3 AI Agent Flow (MCP)

```
User: Check i18n status of this project

→ Tool: audit_i18n
→ Response: {missing: [...], unused: [...], summary: {...}}

User: Which keys are missing in Vietnamese?

→ Tool: get_missing_translations {locales: ["vi"]}
→ Response: [{key: "common.buttons.save", source: "Save", ...}]

User: How do I fix the missing keys?

→ Tool: suggest_translation_fixes {key: "common.buttons.save", target_locales: ["vi"]}
→ Response: {suggestion: "Add to locales/vi/common.json", value: "Lưu", ...}
```

---

## 9. Acceptance Criteria

### 9.1 LSP Server
- [x] Start as Zed language server
- [x] Show inline hints for i18n keys
- [x] Provide hover information with all locales
- [x] Autocomplete keys while typing
- [x] Detect and warn on missing keys
- [x] Support react-i18next, vue-i18n, Laravel, Flutter

### 9.2 CLI
- [x] `audit` command exists and runs through the audit pipeline
- [x] Output supports terminal, JSON, and Markdown formats
- [x] Missing translations are reported
- [x] Unused keys are reported
- [x] Placeholder validation is included in audit reports
- [x] Exit codes reflect issues (0=success, 1=issues found)
- [x] CI controls such as `--fail-on`, baselines, and ignore patterns are implemented

### 9.3 MCP Server
- [x] Implements stdio JSON-RPC transport for MCP clients
- [x] Exposes 4 audit and validation tools
- [x] Returns parseable structured JSON content
- [x] Exposes config, audit, and translation inventory resources
- [ ] Adds patch-producing tools with dry-run defaults

### 9.4 Performance
- Scan 1000+ files in < 10 seconds
- Memory usage < 100MB for typical project
- Sub-100ms response for single-key queries

---

## 10. Future Considerations

### Potential Enhancements
1. **Safe auto-fix**: Add missing keys, remove unused keys through review, sort files, and preserve placeholders
2. **Translation provider integration**: Connect to OpenAI, Anthropic, DeepL, Google Translate, or Azure Translator behind review mode
3. **Git and PR integration**: Auto-detect changed keys and comment on pull requests
4. **Dashboard or desktop manager**: Review coverage, edit translations, and export PR-ready patches
5. **More editors**: VS Code, Neovim, and other LSP clients

### Out of Scope (v1)
- Unreviewed write operations
- Hosted translation management
- Team collaboration features
- Cloud dashboard

---

*Document Version: 1.0*
*Last Updated: 2026-06-29*
