#!/bin/sh
# Records the distribution libraries a plugin program loads, as the
# ffmpeg-x264 ZIP records its own (scripts/package-media-plugins.py): each
# shared object's binary and source package with their exact versions, in
# <licenses>/native/debian-packages.json, and each package's copyright file
# in <licenses>/native/<package>/copyright. Every shared object `ldd` lists
# is recorded except glibc's own (libc, libm, the dynamic loader, ...), the
# platform runtime. Fails when the program needs a library that is not
# installed, when one has no single owning package, or when it does not
# link one of the named libraries.
#
#   record-codec-libraries.sh PROGRAM LICENSES [EXPECTED-LIBRARY...]
#
# The packager's Dockerfile runs it for the standard ffmpeg plugin, which
# must link libopus.so and libvpx.so (from libopus0 and libvpx12); zlib's
# libz.so comes from zlib1g, in the base image.
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
# `ID VERSION_ID`, such as `ubuntu 26.04`, as package-media-plugins.py writes it.
: "${ID:?os-release lacks ID}" "${VERSION_ID:?os-release lacks VERSION_ID}"
for library in "$@"; do
  if ! printf '%s\n' "$listing" | awk -v name="$library" 'index($1, name) == 1 { found = 1 } END { exit !found }'; then
    echo "$program does not link $library" >&2
    exit 1
  fi
done
entries=''
while read -r name path; do
  case "$name" in
    linux-vdso.so.* | ld-linux*.so.* | libc.so.* | libm.so.* | libpthread.so.* | libdl.so.* | librt.so.* | libutil.so.* | libresolv.so.*)
      continue ;;
  esac
  # dpkg records usr-merged paths; try the resolved one first.
  owner="$(dpkg-query --search "$(readlink -f "$path")" 2>/dev/null || dpkg-query --search "$path")"
  owner="${owner%%: *}"
  case "$owner" in
    '' | *,*) echo "no single owner of $path: $owner" >&2; exit 1 ;;
  esac
  fields="$(dpkg-query --show --showformat='"package": "${binary:Package}", "version": "${Version}", "architecture": "${Architecture}", "source_package": "${source:Package}", "source_version": "${source:Version}"' "$owner")"
  entries="$entries${entries:+,
}    { \"library\": \"$name\", $fields }"
  package="$(dpkg-query --show --showformat='${Package}' "$owner")"
  mkdir -p "$native/$package"
  cp "/usr/share/doc/$package/copyright" "$native/$package/copyright"
done <<LIBRARIES
$(printf '%s\n' "$listing" | awk '$2 == "=>" && $3 ~ /^\// { print $1, $3 }')
LIBRARIES
cat > "$native/debian-packages.json" <<JSON
{
  "distribution": "$ID $VERSION_ID",
  "packages": [
$entries
  ],
  "source_instructions": "Retrieve the exact source_package=source_version from this distribution's source archive, including its Debian/Ubuntu patch set; request the matching materials through the bundled source notice (FFMPEG-SOURCE.md)."
}
JSON
