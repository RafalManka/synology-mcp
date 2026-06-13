#!/bin/bash

set -e

VERSION=$(grep '^version' Cargo.toml | head -1 | sed 's/version = "\(.*\)"/\1/')
if [ -z "$VERSION" ]; then
  echo "Error: Could not read version from Cargo.toml."
  exit 1
fi

echo "Building synology-mcp version: $VERSION"

docker buildx build \
  --platform linux/amd64 \
  -t ghcr.io/rafalmanka/synology-mcp:"$VERSION" \
  -t ghcr.io/rafalmanka/synology-mcp:latest \
  --push .

echo "Done: ghcr.io/rafalmanka/synology-mcp:$VERSION pushed."
