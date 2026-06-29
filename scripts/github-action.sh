#!/usr/bin/env bash
set -euo pipefail

version="${INPUT_VERSION:-latest}"
repo="${GITHUB_ACTION_REPOSITORY:-nguyenphutrong/intl-lens}"
install_dir="${RUNNER_TEMP:-/tmp}/intl-lens"
mkdir -p "$install_dir"

case "$(uname -s)" in
  Linux) os="unknown-linux-gnu" ;;
  Darwin) os="apple-darwin" ;;
  MINGW* | MSYS* | CYGWIN*) os="pc-windows-msvc" ;;
  *) echo "Unsupported runner OS: $(uname -s)" >&2; exit 1 ;;
esac

case "$(uname -m)" in
  x86_64 | amd64) arch="x86_64" ;;
  arm64 | aarch64) arch="aarch64" ;;
  *) echo "Unsupported runner architecture: $(uname -m)" >&2; exit 1 ;;
esac

if [[ "$os" == "pc-windows-msvc" ]]; then
  asset="intl-lens-${arch}-${os}.zip"
  binary="$install_dir/intl-lens.exe"
else
  asset="intl-lens-${arch}-${os}.tar.gz"
  binary="$install_dir/intl-lens"
fi

if [[ "$version" == "latest" ]]; then
  url="https://github.com/${repo}/releases/latest/download/${asset}"
else
  url="https://github.com/${repo}/releases/download/${version}/${asset}"
fi

echo "Installing intl-lens from ${url}"
curl --fail --location --silent --show-error "$url" --output "$install_dir/$asset"

if [[ "$asset" == *.zip ]]; then
  unzip -o -q "$install_dir/$asset" -d "$install_dir"
else
  tar -xzf "$install_dir/$asset" -C "$install_dir"
fi

chmod +x "$binary"
echo "$install_dir" >> "$GITHUB_PATH"

args=(
  ci
  --workspace "${INPUT_WORKSPACE:-.}"
  --fail-on "${INPUT_FAIL_ON:-missing,placeholder}"
  --format "${INPUT_FORMAT:-terminal}"
)

if [[ -n "${INPUT_MAX_UNUSED:-}" ]]; then
  args+=(--max-unused "$INPUT_MAX_UNUSED")
fi

if [[ -n "${INPUT_BASELINE:-}" ]]; then
  args+=(--baseline "$INPUT_BASELINE")
fi

if [[ -n "${INPUT_IGNORE_KEY_PATTERN:-}" ]]; then
  args+=(--ignore-key-pattern "$INPUT_IGNORE_KEY_PATTERN")
fi

if [[ -n "${INPUT_IGNORE_FILE:-}" ]]; then
  args+=(--ignore-file "$INPUT_IGNORE_FILE")
fi

if [[ -n "${INPUT_OUTPUT:-}" ]]; then
  args+=(--output "$INPUT_OUTPUT")
fi

if [[ -n "${INPUT_EXTRA_ARGS:-}" ]]; then
  read -r -a extra_args <<< "$INPUT_EXTRA_ARGS"
  args+=("${extra_args[@]}")
fi

"$binary" "${args[@]}"
