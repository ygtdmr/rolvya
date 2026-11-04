#!/usr/bin/env bash
set -euo pipefail
umask 077
cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.."
if [[ $# != 1 || "$1" != /* || -e "$1" ]]; then
  echo 'supply one new absolute distribution directory' >&2
  exit 2
fi
distribution="$1"
[[ -z "$(git status --porcelain --untracked-files=normal)" ]] || { echo 'reviewed clean source required' >&2; exit 2; }
source_commit="$(git rev-parse HEAD)"
for override in RUSTFLAGS CARGO_ENCODED_RUSTFLAGS RUSTC RUSTC_WRAPPER RUSTC_WORKSPACE_WRAPPER NODE_OPTIONS NODE_PATH LD_PRELOAD LD_LIBRARY_PATH BASH_ENV ENV; do
  [[ ! -v "$override" ]] || { echo 'unreviewed build environment override' >&2; exit 2; }
done
while IFS= read -r variable; do
  case "$variable" in
    CARGO_TARGET_DIR) ;;
    HOST_*|TARGET_*|SQLITE_*|CRATE_CC_NO_DEFAULTS|CARGO_CFG_*|DEP_*|OUT_DIR|LIBSQLITE3_*|SQLITE3_*|CARGO_PROFILE_*|CARGO_TARGET_*|CARGO_BUILD_RUST*|CARGO_BUILD_TARGET*|CC|CC_*|CXX|CXX_*|CFLAGS|CFLAGS_*|CXXFLAGS|CXXFLAGS_*|CPPFLAGS|AR|AR_*|LDFLAGS|LDFLAGS_*|RANLIB|PKG_CONFIG|PKG_CONFIG_PATH|PKG_CONFIG_LIBDIR)
      echo 'unreviewed native/compiler build override' >&2; exit 2 ;;
  esac
done < <(compgen -e)
ancestor="$PWD"
while true; do
  for name in .cargo/config .cargo/config.toml; do
    [[ ! -e "$ancestor/$name" && ! -L "$ancestor/$name" ]] || { echo 'unreviewed Cargo ancestor configuration' >&2; exit 2; }
  done
  [[ "$ancestor" != / ]] || break
  ancestor="$(dirname -- "$ancestor")"
done
for configuration in .cargo/config .cargo/config.toml "${CARGO_HOME:-$HOME/.cargo}/config" "${CARGO_HOME:-$HOME/.cargo}/config.toml" apps/web/.env apps/web/.env.local apps/web/.env.production apps/web/.env.production.local; do
  [[ ! -e "$configuration" && ! -L "$configuration" ]] || { echo 'unreviewed local build configuration' >&2; exit 2; }
done
build_target="${CARGO_TARGET_DIR:-$PWD/target}"
[[ "$build_target" == /* ]] || { echo 'absolute build target required' >&2; exit 2; }
cargo build --release --locked -p rolvya-controller -p rolvya-release
(
  cd apps/web
  npm ci --ignore-scripts --no-audit --no-fund
  npm run build
  npm run verify:dist
)
mkdir -m 0700 -- "$distribution"
mkdir -m 0700 -- "$distribution/bin" "$distribution/web" "$distribution/config" "$distribution/systemd"
install -m 0500 "$build_target/release/rolvya-controller" "$distribution/bin/rolvya-controller"
install -m 0400 apps/web/dist/* "$distribution/web/"
install -m 0400 deploy/controller/deployment.example.json "$distribution/config/"
install -m 0400 deploy/controller/rolvya.service "$distribution/systemd/"
commit="$(git rev-parse HEAD)"
[[ "$commit" == "$source_commit" && -z "$(git status --porcelain --untracked-files=normal)" ]] || { echo 'source changed during build' >&2; exit 2; }
binary_digest="$(sha256sum "$distribution/bin/rolvya-controller" | cut -d ' ' -f 1)"
web_digest="$("$distribution/bin/rolvya-controller" asset-digest "$distribution/web")"
printf '{"version":1,"commit":"%s","binary_sha256":"%s","web_sha256":"%s","paid_execution_enabled":false}\n' "$commit" "$binary_digest" "$web_digest" > "$distribution/SOURCE.json"
"$build_target/release/rolvya-release" seal "$PWD" "$distribution"
echo 'build candidate sealed; offline system acceptance and production qualification remain blocked'
