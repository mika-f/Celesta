#!/usr/bin/env bash
# Prints the publishable packages of a Cargo workspace whose version is not on
# crates.io yet, one name per line. The release's crates.io jobs publish only
# these, so a rerun publishes what an earlier attempt left, rather than fail on
# a version crates.io already has.
#
# Usage: scripts/unpublished-crates.sh [path/to/Cargo.toml]
set -euo pipefail

manifest="${1:-Cargo.toml}"
# crates.io refuses API requests without a User-Agent.
user_agent="celesta-release (https://github.com/mika-f/celesta)"

cargo metadata --manifest-path "$manifest" --no-deps --format-version 1 |
  # `publish = false` reads as an empty list of registries.
  jq -r '.packages[] | select(.publish != []) | "\(.name) \(.version)"' |
  while read -r name version; do
    # The API rather than the sparse index, whose CDN can lag behind a version
    # published moments ago.
    status="$(curl -sS -o /dev/null -w '%{http_code}' -A "$user_agent" \
      "https://crates.io/api/v1/crates/$name/$version")"
    case "$status" in
      200) ;;
      404) echo "$name" ;;
      *)
        echo "crates.io answered $status for $name $version" >&2
        exit 1
        ;;
    esac
  done
