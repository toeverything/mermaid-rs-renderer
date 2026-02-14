#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
Run Cargo on a remote machine over SSH, syncing this repository with rsync first.

Usage:
  scripts/remote-cargo.sh <cargo-subcommand> [args...]

Examples:
  scripts/remote-cargo.sh test
  scripts/remote-cargo.sh build --release
  MMDR_REMOTE_HOST=buildbox MMDR_REMOTE_DIR=/tmp/mmdr scripts/remote-cargo.sh clippy --all-targets

Environment:
  MMDR_REMOTE_HOST      Optional. SSH host alias or user@host. Default: desktop
  MMDR_REMOTE_DIR       Optional. Remote checkout dir.
                        Default: .cache/remote-builds/mmdr/<repo-name>
  MMDR_REMOTE_SSH_BIN   Optional. SSH binary path (default: ssh)
  MMDR_REMOTE_RSYNC_BIN Optional. rsync binary path (default: rsync)

Notes:
  - If no Cargo args are provided, this wrapper runs: cargo test
  - Use --wrapper-help for this help text
EOF
}

if [[ "${1:-}" == "--wrapper-help" ]]; then
  usage
  exit 0
fi

if [[ $# -eq 0 ]]; then
  set -- test
fi

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
REPO_NAME="$(basename "$ROOT_DIR")"
MMDR_REMOTE_HOST="${MMDR_REMOTE_HOST:-desktop}"
REMOTE_DIR="${MMDR_REMOTE_DIR:-.cache/remote-builds/mmdr/${REPO_NAME}}"
SSH_BIN="${MMDR_REMOTE_SSH_BIN:-ssh}"
RSYNC_BIN="${MMDR_REMOTE_RSYNC_BIN:-rsync}"

if [[ "$REMOTE_DIR" == *" "* ]]; then
  echo "error: MMDR_REMOTE_DIR cannot contain spaces: $REMOTE_DIR" >&2
  exit 2
fi

for bin in "$SSH_BIN" "$RSYNC_BIN"; do
  if ! command -v "$bin" >/dev/null 2>&1; then
    echo "error: required binary not found: $bin" >&2
    exit 2
  fi
done

printf 'Syncing to %s:%s\n' "$MMDR_REMOTE_HOST" "$REMOTE_DIR"
"$SSH_BIN" "$MMDR_REMOTE_HOST" "$(printf 'mkdir -p %q' "$REMOTE_DIR")"

rsync_args=(
  -az
  --delete
  --exclude=.git/
  --exclude=target/
  --exclude=node_modules/
  --exclude=tmp/
  --exclude=docs/comparisons/
  --exclude=docs/conformance-report/
  --exclude=docs/layout-compare-report/
)

"$RSYNC_BIN" "${rsync_args[@]}" "$ROOT_DIR/" "${MMDR_REMOTE_HOST}:${REMOTE_DIR}/"

remote_cmd=(cargo "$@")
printf -v cmd_str '%q' "${remote_cmd[0]}"
for arg in "${remote_cmd[@]:1}"; do
  printf -v cmd_str '%s %q' "$cmd_str" "$arg"
done
printf -v wrapped_cmd 'cd %q && %s' "$REMOTE_DIR" "$cmd_str"

pretty_args=()
for arg in "$@"; do
  pretty_args+=("$(printf '%q' "$arg")")
done
printf 'Running on %s: cargo %s\n' "$MMDR_REMOTE_HOST" "${pretty_args[*]}"

exec "$SSH_BIN" "$MMDR_REMOTE_HOST" "$wrapped_cmd"
