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
cargo build --release --locked -p rolvya-controller
(
  cd apps/web
  npm ci --ignore-scripts --no-audit --no-fund
  npm run build
  npm run verify:dist
)
mkdir -m 0700 -- "$distribution"
mkdir -m 0700 -- "$distribution/bin" "$distribution/web" "$distribution/config" "$distribution/systemd"
install -m 0500 target/release/rolvya-controller "$distribution/bin/rolvya-controller"
install -m 0400 apps/web/dist/* "$distribution/web/"
install -m 0400 deploy/controller/deployment.example.json "$distribution/config/"
install -m 0400 deploy/controller/rolvya.service "$distribution/systemd/"
commit="$(git rev-parse HEAD)"
binary_digest="$(sha256sum "$distribution/bin/rolvya-controller" | cut -d ' ' -f 1)"
web_digest="$("$distribution/bin/rolvya-controller" asset-digest "$distribution/web")"
printf '{"version":1,"commit":"%s","binary_sha256":"%s","web_sha256":"%s","paid_execution_enabled":false}\n' "$commit" "$binary_digest" "$web_digest" > "$distribution/SOURCE.json"
echo 'distribution built; installation, external PKI and operational qualification remain separate'
