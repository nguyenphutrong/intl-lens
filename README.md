# Intl Lens

Intl Lens is an i18n intelligence layer for codebases.

It runs in editors through LSP, in CI through a CLI, and in AI coding workflows through an MCP server. The Zed extension is the first editor integration, not the whole product.

[Features](#features) | [Install](#install) | [CLI](#cli) | [MCP](#mcp) | [Configure](#configuration) | [Roadmap](ROADMAP.md)

## Features

Intl Lens helps you answer the questions that usually require opening translation files by hand:

- What does this key mean?
- Is this key defined?
- Which locales are missing a value?
- Which translation keys are no longer used?
- Do translated placeholders still match the source locale?

Current surfaces:

| Surface | What it does |
|---------|--------------|
| LSP / Zed | Inline translation hints, hover previews, missing-key diagnostics, autocomplete, go to definition, auto reload |
| CLI | Project audits, file checks, terminal / JSON / Markdown output, CI-friendly exit codes |
| MCP | Agent tools for audit data, missing translations, fix suggestions, and placeholder validation |
| Rust library | Reusable scanner, parser, config, audit, and translation store modules |

## Install

### Zed Extension

1. Open Zed.
2. Go to Extensions with `cmd+shift+x`.
3. Search for `Intl Lens`.
4. Install the extension.

The extension launches the `intl-lens` language server.

### Build from Source

```bash
git clone https://github.com/nguyenphutrong/intl-lens.git
cd intl-lens
cargo build --release
```

This builds:

- `target/release/intl-lens`
- `target/release/intl-lens-cli`
- `target/release/intl-lens-mcp`

Put the binaries on your `PATH` if you want to run them from other projects.

```bash
ln -sf "$(pwd)/target/release/intl-lens" ~/.local/bin/intl-lens
ln -sf "$(pwd)/target/release/intl-lens-mcp" ~/.local/bin/intl-lens-mcp
```

`intl-lens-cli` is still built as a compatibility alias, but the public CLI command is `intl-lens`.

## Editor Usage

When you write code like this:

```tsx
<button>{t("common.actions.submit")}</button>
```

Intl Lens can show the source translation inline, display all locale values on hover, warn when the key is missing, and jump to the translation definition.

Manual Zed configuration example:

```jsonc
{
  "lsp": {
    "intl-lens": {
      "binary": { "path": "intl-lens" }
    }
  },
  "languages": {
    "TSX": {
      "language_servers": ["typescript-language-server", "intl-lens", "..."]
    },
    "TypeScript": {
      "language_servers": ["typescript-language-server", "intl-lens", "..."]
    }
  }
}
```

## CLI

Run a full audit:

```bash
intl-lens audit
```

Write machine-readable output for CI or another tool:

```bash
intl-lens audit --format json --output i18n-report.json
```

Write a Markdown report:

```bash
intl-lens audit --format markdown --output i18n-report.md
```

Check specific files:

```bash
intl-lens check src/components/Checkout.tsx src/pages/Home.tsx
```

Include AI-ready fix suggestions:

```bash
intl-lens audit --suggest-fixes --format json
```

`audit` and `check` return a non-zero exit code when Intl Lens finds missing or unused keys. `ci` uses stricter CI defaults: it fails on missing translations and placeholder mismatches, and it auto-loads `.intl-lens-baseline.json` when that file exists.

CI policy examples:

```bash
intl-lens ci --fail-on missing,placeholder --max-unused 20
intl-lens audit --fail-on missing,unused,placeholder
intl-lens audit --ignore-key-pattern '^legacy\.'
intl-lens ci --ignore-file 'src/generated/**'
```

Baseline flow for projects with existing i18n debt:

```bash
intl-lens audit --write-baseline .intl-lens-baseline.json
intl-lens ci
```

### GitHub Actions Example

```yaml
name: i18n

on:
  pull_request:

jobs:
  audit:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - run: cargo build --release -p intl-lens
      - run: ./target/release/intl-lens ci --format markdown --output i18n-report.md
```

A packaged GitHub Action is planned. See [ROADMAP.md](ROADMAP.md).

## MCP

Start the MCP server from the project you want to inspect:

```bash
intl-lens-mcp
```

Available tools:

| Tool | Purpose |
|------|---------|
| `audit_i18n` | Return missing translations, unused keys, placeholder issues, and summary data |
| `get_missing_translations` | Filter missing keys by locale and optionally include source usage context |
| `suggest_translation_fixes` | Return file targets and source text for a missing key |
| `validate_placeholders` | Check placeholder consistency for one key |

Available resources:

| Resource | Purpose |
|----------|---------|
| `intl-lens://config` | Resolved i18n config |
| `intl-lens://audit/latest` | Fresh audit report |
| `intl-lens://translations/index` | Loaded locales and key count |

Example agent workflow:

```text
Run intl-lens audit, list keys missing in Vietnamese, suggest fixes, then validate placeholders.
```

## Configuration

Intl Lens looks for configuration in this order:

1. `.intl-lens.json`
2. `intl-lens.config.json`
3. `.zed/i18n.json`

Example:

```json
{
  "localePaths": ["src/locales", "public/locales"],
  "sourceLocale": "en",
  "keyStyle": "auto",
  "displayMode": "inlayHints",
  "namespaceEnabled": false
}
```

Options:

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `localePaths` | `string[]` | Common locale directories | Translation directories to scan |
| `sourceLocale` | `string` | `en` | Primary locale used as the source text |
| `keyStyle` | `nested`, `flat`, `auto` | `auto` | Translation key structure |
| `displayMode` | `inlayHints`, `codeLens` | `inlayHints` | LSP display mode |
| `namespaceEnabled` | `boolean` | `false` | Enables namespace-aware behavior |
| `functionPatterns` | `string[]` | Built-in framework patterns | Custom regex patterns for key detection |

Custom pattern example:

```json
{
  "functionPatterns": [
    "t\\s*\\(\\s*[\"']([^\"']+)[\"']",
    "translate\\s*\\(\\s*[\"']([^\"']+)[\"']"
  ]
}
```

## Supported Frameworks

| Framework | Patterns |
|-----------|----------|
| react-i18next | `t("key")`, `useTranslation()`, `<Trans i18nKey="key">` |
| i18next | `t("key")`, `i18n.t("key")` |
| vue-i18n | `$t("key")`, `$tc("key")`, `$te("key")` |
| react-intl | `formatMessage({ id: "key" })` |
| ngx-translate | `translateService.instant("key")`, `translateService.get("key")`, `| translate` |
| Transloco | `translocoService.translate("key")`, `selectTranslate("key")`, `| transloco` |
| Laravel | `__("key")`, `trans("key")`, `Lang::get("key")`, `@lang("key")` |
| Flutter gen_l10n | `AppLocalizations.of(context)!.key` |
| easy_localization | `'key'.tr()`, `tr("key")`, `context.tr("key")` |
| flutter_i18n | `FlutterI18n.translate(context, "key")`, `I18nText("key")` |
| GetX | `'key'.tr`, `'key'.trParams({})` |
| svelte-i18n | `$_("key")`, `$t("key")`, `$format("key")` |
| sveltekit-i18n | `$t("key")`, `t("key")` |

## Supported Source Files

- TypeScript / TSX
- JavaScript / JSX
- HTML
- Angular templates
- PHP
- Blade
- Dart / Flutter
- Vue
- Svelte

## Supported Translation Formats

| Format | Extensions |
|--------|------------|
| JSON | `.json` |
| YAML | `.yaml`, `.yml` |
| PHP | `.php` |
| ARB | `.arb` |

Directory-per-locale:

```text
locales/
  en/
    common.json
  vi/
    common.json
```

Flat files:

```text
locales/
  en.json
  vi.json
```

Flutter ARB:

```text
lib/
  l10n/
    app_en.arb
    app_vi.arb
```

## Development

```bash
cargo test
cargo build
cargo build --release
RUST_LOG=debug ./target/release/intl-lens
```

## Contributing

Good first areas:

- CLI tests for audit and check workflows
- MCP client compatibility tests
- Auto-fix dry-run implementation
- More translation file formats
- Better key detection with AST or tree-sitter

See [ROADMAP.md](ROADMAP.md) for the current product direction.

## License

MIT (c) [Trong Nguyen](https://github.com/nguyenphutrong)
