#!/bin/bash

if [[ $# -eq 0 || "$1" == "help" ]]; then
    echo "Usage: ./build_package.sh [help|all|<targets>]"
    echo ""
    echo "  help: Show this help message."
    echo "  all: Build all targets."
    echo "  <targets>: Build the specified vsix for listed targets, separated by a space. Valid targets are:"
    echo "    win32-x64"
    echo "    win32-arm64"
    echo "    linux-x64"
    echo "    linux-arm64"
    echo "    alpine-x64"
    echo "    alpine-arm64"
    echo "    darwin-x64"
    echo "    darwin-arm64"
    read -n 1 -s -r -p "Press any key to close..."
    exit 0
fi

PACKAGE_VERSION=$(cat package.json \
  | grep version \
  | head -1 \
  | awk -F: '{ print $2 }' \
  | sed 's/[",]//g')
echo "detected version: $PACKAGE_VERSION"
version_list=(${PACKAGE_VERSION//./ })
if {
    [[ $(( ${version_list[0]} )) -eq 0 && $(( ${version_list[1]} % 2 )) -eq 0 ]] || \
    [[ $(( ${version_list[1]} % 2 )) -eq 1 && $(( ${version_list[0]} )) -ne 0 ]]
}; then
  echo "pre-release version $PACKAGE_VERSION"
  nox --session build_package_prerelease -- $PACKAGE_VERSION "$@"
else
  echo "release version $PACKAGE_VERSION"
  nox --session build_package -- $PACKAGE_VERSION "$@"
fi

read -n 1 -s -r -p "Press any key to close..."