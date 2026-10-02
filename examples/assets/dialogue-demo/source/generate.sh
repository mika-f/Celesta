#!/usr/bin/env bash
# Regenerates the dialogue demo's portraits and voices.
#
# Requirements:
#   - Open JTalk with the NAIST JDIC dictionary (Debian/Ubuntu:
#     `apt-get install open-jtalk open-jtalk-mecab-naist-jdic`).
#   - The MMDAgent "Mei" and "Takumi" HTS voices (CC BY 3.0), from
#     MMDAgent_Example-1.8 (https://sourceforge.net/projects/mmdagent/).
#     Set MMDAGENT_VOICES to its `Voice` directory.
#   - ffmpeg, node, and python3 with Pillow on PATH, and a built celesta-exporter
#     (`cargo build -p celesta-exporter --release` from the repo root).
set -euo pipefail

here="$(cd "$(dirname "$0")" && pwd)"
demo="$(dirname "$here")"
root="$(cd "$demo/../../.." && pwd)"
voices="${MMDAGENT_VOICES:?set MMDAGENT_VOICES to MMDAgent_Example-1.8/Voice}"
dic="${OPEN_JTALK_DIC:-/var/lib/mecab/dic/open-jtalk/naist-jdic}"
exporter="${CELESTA_EXPORTER:-$root/target/release/celesta-exporter}"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

# Portraits: draw-portraits.tsx draws each image on black, then on white;
# matte.py turns each pair into one transparent PNG.
mkdir -p "$demo/portraits"
names=$(node -e "
  const src = require('fs').readFileSync(process.argv[1], 'utf8');
  const list = src.match(/IMAGES = \[([^\]]*)\]/)[1];
  console.log([...list.matchAll(/'([^']+)'/g)].map((m) => m[1]).join(' '));
" "$here/draw-portraits.tsx")
count=$(echo "$names" | wc -w)
frames=$(seq -s, 0 $((count * 2 - 1)))
"$exporter" --overwrite --react "$here/draw-portraits.tsx" --frames "$frames" "$tmp/portrait.png"
i=0
for name in $names; do
  python3 "$here/matte.py" \
    "$(printf '%s/portrait-%06d.png' "$tmp" "$i")" \
    "$(printf '%s/portrait-%06d.png' "$tmp" $((i + count)))" \
    "$demo/portraits/$name.png"
  i=$((i + 1))
done

# Voices: Shizuku speaks with Mei, Komugi with Takumi; smiling lines use
# the voice's "happy" style.
mkdir -p "$demo/voices"
node -e "
  for (const line of JSON.parse(require('fs').readFileSync(process.argv[1], 'utf8'))) {
    const voice = line.speaker === 'shizuku' ? 'mei/mei' : 'takumi/takumi';
    const style = line.expression === 'smile' ? 'happy' : 'normal';
    console.log([line.id, voice + '_' + style + '.htsvoice', line.speech ?? line.text].join('\t'));
  }
" "$demo/script.json" | while IFS=$'\t' read -r id voice text; do
  printf '%s\n' "$text" > "$tmp/$id.txt"
  open_jtalk -x "$dic" -m "$voices/$voice" -ow "$tmp/$id.wav" "$tmp/$id.txt"
  ffmpeg -nostdin -v error -y -i "$tmp/$id.wav" -ar 24000 -ac 1 -c:a pcm_s16le "$demo/voices/$id.wav"
done
