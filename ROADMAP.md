# Intl Lens Roadmap

## Positioning

Intl Lens is an i18n intelligence layer for codebases.

The product should work across four surfaces:

| Surface | Role |
|---------|------|
| Editor / LSP | Give developers inline context while they code |
| CLI / CI | Audit translation health and fail builds when needed |
| MCP / agents | Give AI coding agents structured i18n context and safe patch workflows |
| UI / dashboard | Help developers, QA, product, and translators review coverage and edit translations |

The Zed extension remains an important distribution channel, but the product should not be limited to "a Zed extension."

## Current State

Workspace version: `0.1.6`

| Area | Status | Notes |
|------|--------|-------|
| Rust workspace | Done | `intl-lens` core crate plus Zed extension crate |
| LSP server | Done | Inline hints, hover, diagnostics, autocomplete, go to definition, reload |
| CLI audit | Usable | `audit`, `check`, JSON, Markdown, terminal output, CI exit codes |
| CLI fix | Stub | `fix --dry-run` exists but only prints a placeholder message |
| MCP server | Usable | Tools and resources are implemented over stdio JSON-RPC |
| Audit model | Usable | Missing translations, unused keys, placeholder issues, fix suggestions |
| Config | Usable | `.i18n-ally.json`, `i18n-ally.config.json`, `.zed/i18n.json` |
| File formats | Partial | JSON, YAML, PHP, ARB |
| Key detection | Regex-based | Broad framework coverage, but dynamic keys need better classification |

## Guiding Principles

- Keep the Rust core reusable. Editor, CLI, MCP, and UI surfaces should share scanner, parser, config, store, and audit code.
- Prefer dry-run and patch output before write operations.
- Make CI behavior explicit. A team should know exactly why a build failed.
- Treat placeholders as correctness constraints, not translation suggestions.
- Build narrow MVPs before provider integrations or AI-heavy workflows.

## Priority Roadmap

### P0: Make CLI Audit Production-Ready for CI

Goal: teams can add Intl Lens to a pull request workflow without custom glue code.

Planned work:

- Add `--fail-on missing,unused,placeholder`.
- Add `--ignore-key-pattern`.
- Add `--ignore-file`.
- Add `--baseline .intl-lens-baseline.json` for existing projects with known debt.
- Add `ci` command as a CI-oriented alias with stricter defaults.
- Document GitHub Actions and GitLab CI examples.
- Add integration tests for exit codes and report formats.

Target examples:

```bash
intl-lens-cli audit --format json
intl-lens-cli audit --format markdown --output i18n-report.md
intl-lens-cli ci --fail-on missing,placeholder --max-unused 20
```

Success criteria:

- CI can fail on selected issue classes.
- Existing projects can adopt the tool with a baseline.
- JSON and Markdown outputs stay stable enough for automation.

### P1: Implement Safe Auto-Fix

Goal: turn audit suggestions into reviewable file changes.

Planned work:

- Implement `fix --dry-run`.
- Add `--add-missing`.
- Add `--placeholder "_TODO_"`.
- Add `--remove-unused --interactive`.
- Add `--sort-keys`.
- Preserve file format and minimize diff noise.
- Add tests for JSON, YAML, PHP, and ARB write paths before broad rollout.

Target examples:

```bash
intl-lens-cli fix --dry-run
intl-lens-cli fix --add-missing --placeholder "_TODO_"
intl-lens-cli fix --remove-unused --interactive
intl-lens-cli fix --sort-keys
```

Fix behavior:

- Missing key: add it to the target locale file with source text, `_TODO_`, or an empty value.
- Unused key: remove, keep, or ignore through an interactive review.
- Placeholder mismatch: copy placeholder shape from the source locale to the target locale.
- Sort keys: produce stable ordering.

Success criteria:

- Dry-run output shows exact files and keys.
- Write mode is covered by tests.
- Fixes reuse `FixSuggestion` instead of introducing a second model.

### P1: Expand MCP into an Agent Toolkit

Goal: AI agents can inspect, plan, and propose i18n patches without scraping files manually.

Current tools:

- `audit_i18n`
- `get_missing_translations`
- `suggest_translation_fixes`
- `validate_placeholders`

Planned tools:

- `translate_missing_keys`
- `apply_translation_patch`
- `extract_hardcoded_strings`
- `review_i18n_pr`
- `get_translation_context`

Safety rules:

- `apply_translation_patch` should default to `dry_run=true`.
- Patch-producing tools should return unified diffs before mutating files.
- Translation tools must preserve placeholders, ICU syntax, HTML tags, and Markdown tags.

Example workflow:

```text
Run intl-lens audit, translate missing Vietnamese keys from English, return a patch, then validate placeholders.
```

Success criteria:

- MCP responses include structured data, not only text.
- Agents can identify target files and source usage context.
- Mutating tools are opt-in and test-covered.

### P2: Extract Hardcoded Strings

Goal: move untranslated user-facing strings into translation files.

Initial command:

```bash
intl-lens-cli extract src/components/Checkout.tsx --locale en --namespace checkout
```

Example transformation:

```tsx
<button>Submit order</button>
```

becomes:

```tsx
<button>{t("checkout.submitOrder")}</button>
```

and `en.json` receives:

```json
{
  "checkout": {
    "submitOrder": "Submit order"
  }
}
```

MVP scope:

- TSX JSX text.
- Vue template text.
- Laravel Blade text.
- Flutter `Text("...")`.

Later scope:

- Key naming heuristics from file path, component name, tag, and nearby labels.
- AI-generated keys behind review mode.
- Batch extraction with unified diff output.

### P2: Namespace and Monorepo Support

Goal: support larger workspaces with multiple apps and locale roots.

Planned config shape:

```json
{
  "projects": [
    {
      "name": "admin",
      "root": "apps/admin",
      "localePaths": ["src/locales"],
      "sourceLocale": "en"
    },
    {
      "name": "mini-app",
      "root": "apps/mini-app",
      "localePaths": ["src/i18n"],
      "sourceLocale": "vi"
    }
  ]
}
```

Planned commands:

```bash
intl-lens-cli audit --project admin
intl-lens-cli audit --all-projects
```

Success criteria:

- Reports include project names.
- Baselines and ignore rules can be scoped per project.
- CLI and MCP expose the same project model.

### P2: Smarter Key Detection

Goal: reduce false positives and classify dynamic keys.

Planned engine:

1. Regex fast path for broad framework coverage.
2. Optional AST or tree-sitter parser for TSX, Vue, Svelte, PHP, and Dart.

Classification:

- Static key: check exactly.
- Template key: check prefix or known variants.
- Dynamic key: warn and suggest an allowlist.

Planned config:

```json
{
  "dynamicKeyPolicy": "warn",
  "allowedDynamicPrefixes": ["checkout.status."]
}
```

Success criteria:

- Dynamic keys no longer appear as ordinary missing keys.
- Reports explain whether a finding is exact, prefix-based, or dynamic.

### P3: Local Web Dashboard

Goal: make audit data easier to review than static JSON or Markdown.

MVP route:

- Keep Rust core unchanged.
- Generate audit JSON through the CLI.
- Build a local web app that reads the JSON report.

Dashboard views:

- Coverage by locale.
- Missing keys table.
- Unused keys table.
- Placeholder mismatch table.
- Key search.
- Source usage with file, line, and snippet.
- PR-ready patch export.

Later route:

- Local HTTP server for live project scanning.
- Tauri desktop shell if write workflows and non-developer users become central.

### P3: Desktop i18n Manager

Goal: support QA, content, product, and translators who need a GUI.

Candidate stack: Tauri plus the existing Rust core.

Feature ideas:

- Open a project folder.
- Auto-detect locale paths.
- Show a translation matrix with keys as rows and locales as columns.
- Edit multiple locales side by side.
- Validate placeholders in real time.
- Import and export CSV or XLSX.
- Generate PR-ready patches.

This should wait until CLI write operations and dashboard data models are stable.

### P3: Translation Provider Integrations

Goal: fill missing translations with reviewable provider output.

Planned command shape:

```bash
intl-lens-cli translate --provider openai --from en --to vi,ja
intl-lens-cli translate --provider deepl --missing-only
```

Candidate providers:

- OpenAI, Anthropic, or local LLM.
- DeepL.
- Google Translate.
- Azure Translator.

Required guardrails:

- Preserve placeholders such as `{name}`, `{{count}}`, `%s`, and `:name`.
- Preserve ICU syntax.
- Preserve HTML and Markdown tags.
- Support a glossary file.
- Default to review mode before writing.

Glossary example:

```json
{
  "POS": "POS",
  "checkout": "thanh toán",
  "voucher": "voucher"
}
```

### P3: GitHub Action and PR Reviewer

Goal: make Intl Lens easy to adopt and demo in pull requests.

Target Action:

```yaml
- uses: nguyenphutrong/intl-lens-action@v1
  with:
    fail-on: missing,placeholder
    comment-pr: true
```

PR comment should include:

- New keys that lack target locales.
- Deleted keys that still exist in translation files.
- Placeholder mismatches.
- Coverage change from the base branch.

The existing Markdown report is a good starting point for this feature.

### P4: More Translation Formats

Goal: broaden adoption outside web-only projects.

Planned formats:

- `.po` and `.pot` for gettext, WordPress, and Django.
- `.xlf` and `.xliff` for Angular, iOS, and localization tools.
- `.toml` for Rust and config-heavy projects.
- Android `strings.xml`.
- iOS `.strings` and `.stringsdict`.

Success criteria:

- Parser and writer behavior is tested per format.
- Placeholder extraction supports the format's conventions.
- Round-trip formatting does not create noisy diffs.

### P4: Translation Quality Checks

Goal: catch translation problems beyond missing keys.

Candidate checks:

- Empty string.
- Duplicate translation values that look accidental.
- Key naming convention violations.
- Placeholder-like raw values.
- Mobile max-length limits.
- HTML tag consistency.
- Markdown tag consistency.

These checks should be configurable so teams can adopt them gradually.

## Release Plan

| Milestone | Focus |
|-----------|-------|
| `0.2.0` | Documented CLI and MCP, CI-ready audit improvements, tests |
| `0.3.0` | Safe auto-fix dry-run and write mode |
| `0.4.0` | Expanded MCP agent toolkit |
| `0.5.0` | Extraction MVP |
| `0.6.0` | Monorepo and namespace support |
| `1.0.0` | Stable CLI/MCP contracts, production docs, packaged integrations |

## Near-Term Checklist

- [ ] Add CLI tests for `audit`, `check`, output formats, and exit codes.
- [ ] Add MCP integration tests for all tools and resources.
- [ ] Implement `--fail-on`.
- [ ] Implement baseline file support.
- [ ] Replace `fix` stub with dry-run output.
- [ ] Add README examples that are tested in CI.

Last updated: 2026-06-29
