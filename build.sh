#!/usr/bin/env bash

set -euo pipefail

ODOOLS_PYCHARM_DIR="${ODOOLS_PYCHARM_DIR:-../odoo-ls-pycharm}"
ODOOLS_VSCODE_DIR="${ODOOLS_VSCODE_DIR:-../odoo-ls-vscode}"

if [ ! -d "$ODOOLS_PYCHARM_DIR" ]; then
  echo "Error: Folder '$ODOOLS_PYCHARM_DIR' doesn't exist. Be sure to launch this script from the root of the repository or set ODOOLS_PYCHARM_DIR accordingly."
  exit 1
fi

if [ ! -d "$ODOOLS_VSCODE_DIR" ]; then
  echo "Error: Folder '$ODOOLS_VSCODE_DIR' doesn't exist. Be sure to launch this script from the root of the repository or set ODOOLS_VSCODE_DIR accordingly."
  exit 1
fi

VALID_TARGETS=("lsp" "pycharm" "vscode")

if [ $# -eq 0 ]; then
  echo "Usage: $0 [all|${VALID_TARGETS[*]}...]"
  exit 1
fi

run_build() {
  local target=$1
  case "$target" in
    lsp)
      echo "=== Building LSP ==="
      (cd lsp && ./build.sh)
      ;;
    pycharm)
      echo "=== Building PyCharm Plugin ==="
      (cd "$PYCHARM_DIR" && ./build.sh)
      ;;
    vscode)
      echo "=== Building VSCode Extension ==="
      (cd "$VSCODE_DIR" && ./build.sh)
      ;;
    *)
      echo "Unknown target: $target"
      exit 1
      ;;
  esac
}

if [ "$1" == "all" ]; then
  for t in "${VALID_TARGETS[@]}"; do
    run_build "$t"
  done
else
  for arg in "$@"; do
    run_build "$arg"
  done
fi