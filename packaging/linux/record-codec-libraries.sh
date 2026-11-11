#!/bin/sh
# Records the distribution codec libraries a plugin program loads, as the
# ffmpeg-x264 ZIP records its own (scripts/package-media-plugins.py): each
# shared object's binary and source package with their exact versions, in
# <licenses>/native/debian-packages.json, and each package's copyright file
# in <licenses>/native/<package>/copyright. Fails when the program needs a
# library that is not installed, or does not link one of the named libraries.
#
#   record-codec-libraries.sh PROGRAM LICENSES LIBRARY...
#
# The packager's Dockerfile runs it for the standard ffmpeg plugin with
# libopus.so and libvpx.so, which the image installs from libopus0 and libvpx9.
set -eu

program="$1"
native="$2/native"
shift 2

listing="$(ldd "$program")"
if printf '%s\n' "$listing" | grep -q 'not found'; then
  printf '%s: unresolved libraries:\n%s\n' "$program" "$listing" >&2
  exit 1
fi
mkdir -p "$native"
. /etc/os-release
entries=''
for library in "$@"; do
  path="$(printf '%s\n' "$listing" | awk -v name="$library" 'index($1, name) == 1 { print $3; exit }')"
  if [ -z "$path" ]; then
    echo "$program does not link $library" >&2
    exit 1
  fi
  name="$(basename "$path")"
  # dpkg records usr-merged paths; try the resolved one first.
  owner="$(dpkg-query --search "$(readlink -f "$path")" 2>/dev/null || dpkg-query --search "$path")"
  owner="${owner%%: *}"
  case "$owner" in
    *,*) echo "ambiguous owner of $path: $owner" >&2; exit 1 ;;
  esac
  fields="$(dpkg-query --show --showformat='"package": "${binary:Package}", "version": "${Version}", "architecture": "${Architecture}", "source_package": "${source:Package}", "source_version": "${source:Version}"' "$owner")"
  entries="$entries${entries:+,
}    { \"library\": \"$name\", $fields }"
  package="$(dpkg-query --show --showformat='${Package}' "$owner")"
  mkdir -p "$native/$package"
  cp "/usr/share/doc/$package/copyright" "$native/$package/copyright"
done
cat > "$native/debian-packages.json" <<JSON
{
  "distribution": "$ID $VERSION_ID",
  "packages": [
$entries
  ],
  "source_instructions": "Retrieve the exact source_package=source_version from this distribution's source archive, including its Debian/Ubuntu patch set; request the matching materials through the bundled source notice (FFMPEG-SOURCE.md)."
}
JSON
