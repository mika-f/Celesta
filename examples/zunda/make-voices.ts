// script.ts の各行を VOICEVOX Engine で読み上げて voices/<id>.wav を作り、
// 読みがな（口パク用）と長さを voices.json に書き出す。
//
//   docker run -d -p 50021:50021 voicevox/voicevox_engine:cpu-ubuntu24.04-latest
//   node examples/zunda/make-voices.ts
//
// VOICEVOX_URL で Engine の場所を変えられる。既にある WAV は、台本の
// 文・スタイルが変わっていなければ作り直さない。

import { createHash } from 'node:crypto';
import { existsSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { LINES } from './script.ts';

const ENGINE = process.env.VOICEVOX_URL ?? 'http://127.0.0.1:50021';
const DIR = import.meta.dirname;
const VOICES = join(DIR, 'voices');
const MANIFEST = join(DIR, 'voices.json');

export type VoiceInfo = { file: string; kana: string; seconds: number; hash: string };

const SPEED = 1.1;

/** VOICEVOX の AudioQuery のうち、ここで読み書きする項目。残りはそのまま /synthesis に返す。 */
type AudioQuery = {
  kana: string;
  speedScale: number;
  outputSamplingRate: number;
  prePhonemeLength: number;
  postPhonemeLength: number;
};

async function engine(path: string, init?: RequestInit): Promise<Response> {
  const response = await fetch(`${ENGINE}${path}`, init);
  if (!response.ok) throw new Error(`${path}: HTTP ${response.status} ${await response.text()}`);
  return response;
}

// PCM WAV の長さ（秒）。fmt と data チャンクだけ読む。
function wavSeconds(bytes: Uint8Array): number {
  const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  let byteRate = 0;
  for (let at = 12; at + 8 <= bytes.length; ) {
    const id = String.fromCharCode(...bytes.subarray(at, at + 4));
    const size = view.getUint32(at + 4, true);
    if (id === 'fmt ') byteRate = view.getUint32(at + 16, true);
    if (id === 'data') return size / byteRate;
    at += 8 + size + (size & 1);
  }
  throw new Error('no data chunk');
}

const previous: Record<string, VoiceInfo> = existsSync(MANIFEST) ? JSON.parse(readFileSync(MANIFEST, 'utf8')) : {};
const manifest: Record<string, VoiceInfo> = {};
mkdirSync(VOICES, { recursive: true });

for (const line of LINES) {
  const say = line.say ?? line.text;
  const hash = createHash('sha1').update(`${line.style}:${SPEED}:${say}`).digest('hex').slice(0, 12);
  const file = `voices/${line.id}.wav`;
  const path = join(DIR, file);
  if (previous[line.id]?.hash === hash && existsSync(path)) {
    manifest[line.id] = previous[line.id];
    continue;
  }
  const query = (await (await engine(`/audio_query?speaker=${line.style}&text=${encodeURIComponent(say)}`, { method: 'POST' })).json()) as AudioQuery;
  query.speedScale = SPEED;
  query.outputSamplingRate = 48000;
  query.prePhonemeLength = 0.05;
  query.postPhonemeLength = 0.1;
  const wav = new Uint8Array(await (await engine(`/synthesis?speaker=${line.style}`, {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify(query),
  })).arrayBuffer());
  writeFileSync(path, wav);
  manifest[line.id] = { file: `./${file}`, kana: query.kana, seconds: wavSeconds(wav), hash };
  console.log(`${line.id}  ${manifest[line.id].seconds.toFixed(2)}s  ${say}`);
}

writeFileSync(MANIFEST, `${JSON.stringify(manifest, null, 2)}\n`);
const total = Object.values(manifest).reduce((s, v) => s + v.seconds, 0);
console.log(`${LINES.length} lines, ${total.toFixed(1)}s of speech`);
