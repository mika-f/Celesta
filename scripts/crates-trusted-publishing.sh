#!/usr/bin/env bash
# Sets up crates.io Trusted Publishing for every publishable crate of both
# workspaces (Cargo.toml and plugins/Cargo.toml): crates.io then accepts
# releases from .github/workflows/package.yml in the crates-io environment of
# mika-f/Celesta. A crate that already has that configuration is left as it is.
#
# Run it as an owner of the crates, with an API token that has the
# "trusted-publishing" endpoint scope (crates.io: Account Settings > API
# Tokens), and with a verified email address and a linked GitHub account:
#
#   CRATES_IO_TOKEN=... scripts/crates-trusted-publishing.sh
#
# With --trustpub-only, it also has each crate refuse new versions from API
# tokens, so only the release workflow can publish them.
set -euo pipefail

owner="mika-f"
repository="Celesta"
workflow="package.yml"
environment="crates-io"
api="https://crates.io/api/v1"
user_agent="celesta-release (https://github.com/mika-f/celesta)"

trustpub_only=false
case "${1:-}" in
  "") ;;
  --trustpub-only) trustpub_only=true ;;
  *)
    echo "usage: $0 [--trustpub-only]" >&2
    exit 2
    ;;
esac

: "${CRATES_IO_TOKEN:?Set CRATES_IO_TOKEN to a crates.io API token with the trusted-publishing scope}"

root="$(cd "$(dirname "$0")/.." && pwd)"

request() {
  local method="$1" path="$2"
  shift 2
  curl -sS --fail-with-body -X "$method" -A "$user_agent" \
    -H "Authorization: $CRATES_IO_TOKEN" -H "Content-Type: application/json" \
    "$@" "$api$path"
}

crates="$(
  for manifest in "$root/Cargo.toml" "$root/plugins/Cargo.toml"; do
    cargo metadata --manifest-path "$manifest" --no-deps --format-version 1 |
      jq -r '.packages[] | select(.publish != []) | .name'
  done
)"

for crate in $crates; do
  if ! configs="$(request GET "/trusted_publishing/github_configs?crate=$crate")"; then
    echo "$crate: could not list its configurations (is it on crates.io, and are you an owner?)" >&2
    echo "$configs" >&2
    exit 1
  fi
  if jq -e --arg owner "$owner" --arg repo "$repository" --arg workflow "$workflow" --arg env "$environment" \
    'any(.github_configs[]; (.repository_owner | ascii_downcase) == ($owner | ascii_downcase)
      and (.repository_name | ascii_downcase) == ($repo | ascii_downcase)
      and .workflow_filename == $workflow and .environment == $env)' <<< "$configs" > /dev/null; then
    echo "$crate: already configured"
  else
    body="$(jq -n --arg crate "$crate" --arg owner "$owner" --arg repo "$repository" \
      --arg workflow "$workflow" --arg env "$environment" \
      '{github_config: {crate: $crate, repository_owner: $owner, repository_name: $repo,
        workflow_filename: $workflow, environment: $env}}')"
    request POST /trusted_publishing/github_configs --data "$body" > /dev/null
    echo "$crate: configured"
  fi
  if [[ "$trustpub_only" == true ]]; then
    request PATCH "/crates/$crate" --data '{"crate": {"trustpub_only": true}}' > /dev/null
    echo "$crate: trusted publishing only"
  fi
done
