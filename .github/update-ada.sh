#!/bin/sh
set -e

BASE_DIR=$(pwd)
DEPENDENCIES_DIR="$BASE_DIR/deps"

WORKSPACE=$(mktemp -d 2> /dev/null || mktemp -d -t 'tmp')

cleanup () {
  EXIT_CODE=$?
  [ -d "$WORKSPACE" ] && rm -rf "$WORKSPACE"
  exit $EXIT_CODE
}

trap cleanup INT TERM EXIT

cd "$WORKSPACE"
curl -sL -o "ada" "https://github.com/ada-url/ada/releases/latest/download/singleheader.zip"
unzip ada
echo "$DEPENDENCIES_DIR"
cp ada.h ada.cpp "$DEPENDENCIES_DIR"
