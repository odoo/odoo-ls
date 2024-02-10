#!/bin/bash

PACKAGE_VERSION=$(cat package.json \
  | grep version \
  | head -1 \
  | awk -F: '{ print $2 }' \
  | sed 's/[",]//g')
echo "detected version: $PACKAGE_VERSION"
middle_version=(${PACKAGE_VERSION//./ })
if [ $(( ${middle_version[1]} % 2 )) -eq 0 ]; then
  echo "pre-release version"
  nox --session build_package_prerelease
else
  echo "release version"
  nox --session build_package
fi
