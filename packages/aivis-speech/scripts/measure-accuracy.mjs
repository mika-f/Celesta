// Measures timingFromAivisSpeech against voices whose true phoneme times are
// known: VOICEVOX Engine WAVs, timed exactly by timingFromVoicevox. Each
// query is turned into what AivisSpeech returns (no lengths, each pause a
// ',' mora, and the interrogative upspeak mora VOICEVOX appends spelled
// out, since the WAV contains it), then timed from the WAV alone.
//
//   docker run --rm -p 50021:50021 voicevox/voicevox_engine:cpu-ubuntu24.04-0.25.2
//   pnpm --filter "@celesta/cli..." run build
//   node packages/aivis-speech/scripts/measure-accuracy.mjs [--lines] [--cache <dir>]
//
// VOICEVOX_URL overrides http://127.0.0.1:50021. Synthesized voices are
// kept in --cache (default: a temporary directory) so reruns skip the engine.
// Every corpus line is read by every voice in VOICES at each speed in SPEEDS.

import { existsSync, mkdirSync, mkdtempSync, readFileSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { parseArgs } from 'node:util';

import { buildEnvelope, decodeWav, lipSyncTimeline } from '@celesta/character';
import { lipSyncFromVoicevox, timingFromVoicevox } from '../../voicevox/dist/index.js';
import { lipSyncFromAivisSpeech, timingFromAivisSpeech } from '../dist/index.js';

const ENGINE = (process.env.VOICEVOX_URL ?? 'http://127.0.0.1:50021').replace(/\/$/, '');
// 四国めたん, ずんだもん, 春日部つむぎ, 玄野武宏, 青山龍星, 冥鳴ひまり (normal styles).
const VOICES = [2, 3, 8, 11, 13, 14];
const SPEEDS = [1, 1.12, 1.25];
// General sentences, then the lines of examples/shorts-explainer (as read).
const CORPUS = [
  'こんにちは、ずんだもんなのだ。',
  '本日は晴天なり。',
  'みなさん、元気ですか？今日も一日がんばりましょう。',
  'カメラを左に動かして、ゆっくりズームします。',
  'ぴったりの答えが、きっと見つかるはずです。',
  'えーと、それは、つまり、どういうことでしょうか。',
  '一つ目のポイントは速さ、二つ目は正確さです。',
  'このライブラリを使えば、口の動きを音声に合わせられます。',
  'この動画、じつは全部コードです。',
  'えっ、編集ソフトじゃないの？',
  'セレスタは、リアクトとタイプスクリプトで動画を書くツールです。',
  'ウェブページみたいに？',
  'はい。四角や文字の部品を組み合わせて作ります。',
  'タイムラインはどこにあるの？',
  'フレーム番号から絵を計算する関数。それだけです。',
  'じゃあ、わたしたちの声は？',
  'ボイスボックスの音素の長さから、口パクを作っています。',
  'まばたきや表情も？',
  'ピーエスディーのレイヤーを、フレームごとに切り替えています。',
  '台本を直したら、作り直し？',
  '声を作り直すだけ。映像は音声の長さに合わせて動きます。',
  '動画も、コードで書けるんだね。',
  'セレスタで、作ってみてください。',
];

const { values: args } = parseArgs({ options: { lines: { type: 'boolean' }, cache: { type: 'string' } } });
const cache = args.cache ?? mkdtempSync(join(tmpdir(), 'aivis-speech-accuracy-'));
mkdirSync(cache, { recursive: true });

async function post(path, params, body) {
  const response = await fetch(`${ENGINE}${path}?${new URLSearchParams(params)}`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: body === undefined ? undefined : JSON.stringify(body),
  });
  if (!response.ok) throw new Error(`${path}: ${response.status} ${await response.text()}`);
  return response;
}

async function voice(text, speaker, speedScale) {
  const name = join(cache, `${speaker}-${speedScale}-${CORPUS.indexOf(text)}`);
  if (!existsSync(`${name}.wav`)) {
    const query = await (await post('/audio_query', { speaker, text })).json();
    query.speedScale = speedScale;
    const wav = new Uint8Array(await (await post('/synthesis', { speaker }, query)).arrayBuffer());
    writeFileSync(`${name}.json`, JSON.stringify(query));
    writeFileSync(`${name}.wav`, wav);
  }
  return { query: JSON.parse(readFileSync(`${name}.json`, 'utf8')), wav: readFileSync(`${name}.wav`) };
}

// What AivisSpeech would return for the moras VOICEVOX spoke.
function asAivisSpeech(query, truth) {
  const blank = (mora) => ({
    text: mora.text, consonant: mora.consonant ?? null, consonant_length: mora.consonant ? 0 : null,
    vowel: mora.vowel, vowel_length: 0, pitch: 0,
  });
  return {
    ...query,
    accent_phrases: query.accent_phrases.map((phrase, i) => ({
      moras: [
        ...truth.accentPhrases[i].moras.map((m) => blank({ text: m.text, consonant: m.consonant?.phoneme, vowel: m.vowel.phoneme })),
        ...(phrase.pause_mora ? [blank({ text: ',', vowel: 'pau' })] : []),
      ],
      accent: phrase.accent, pause_mora: null, is_interrogative: false,
    })),
  };
}

const errors = [];
let samples = 0;
let estimatedMatches = 0;
let waveformMatches = 0;
let milliseconds = 0;
let voiceSeconds = 0;
for (const speaker of VOICES) {
  for (const [index, text] of CORPUS.entries()) {
    const speedScale = SPEEDS[(index + speaker) % SPEEDS.length];
    const { query, wav } = await voice(text, speaker, speedScale);
    const truth = timingFromVoicevox(query);
    const started = performance.now();
    const estimate = timingFromAivisSpeech(asAivisSpeech(query, truth), wav);
    milliseconds += performance.now() - started;
    voiceSeconds += truth.durationInSeconds;

    const spoken = estimate.moras.filter((m) => m.vowel.phoneme !== 'pau');
    const lineErrors = truth.moras.map((m, i) => Math.abs(m.start - spoken[i].start));
    errors.push(...lineErrors);

    const exact = lipSyncFromVoicevox(truth);
    const estimated = lipSyncFromAivisSpeech(estimate);
    const waveform = lipSyncTimeline(buildEnvelope(decodeWav(wav), 100), truth.moras.map((m) => m.text).join(''));
    let lineMatches = 0;
    let lineSamples = 0;
    for (let seconds = 0.005; seconds < truth.durationInSeconds; seconds += 0.01) {
      const mouth = exact.mouthAtSeconds(seconds);
      lineSamples += 1;
      if (estimated.mouthAtSeconds(seconds) === mouth) lineMatches += 1;
      if ((waveform[Math.floor(seconds * 100)] ?? 'closed') === mouth) waveformMatches += 1;
    }
    samples += lineSamples;
    estimatedMatches += lineMatches;
    if (args.lines) {
      const worst = Math.max(...lineErrors) * 1000;
      console.log(`${speaker} ×${speedScale} ${text}  worst ${worst.toFixed(0)} ms, mouth ${(100 * lineMatches / lineSamples).toFixed(1)}%`);
    }
  }
}

errors.sort((a, b) => a - b);
const quantile = (q) => `${(errors[Math.floor(q * (errors.length - 1))] * 1000).toFixed(0)} ms`;
const percent = (count, total) => `${((100 * count) / total).toFixed(1)}%`;
console.log(`VOICEVOX Engine ${await (await fetch(`${ENGINE}/version`)).json()}, voices ${VOICES.join(', ')}, speeds ${SPEEDS.join(', ')}`);
console.log(`${VOICES.length * CORPUS.length} lines, ${errors.length} moras, ${voiceSeconds.toFixed(0)} s of voice`);
console.log(`mora start error: median ${quantile(0.5)}, p90 ${quantile(0.9)}, max ${quantile(1)}`);
console.log(`moras within one 30 fps frame: ${percent(errors.filter((e) => e < 1 / 30).length, errors.length)}`);
console.log(`mouth matches the true mouth: ${percent(estimatedMatches, samples)} (loadLipSync's method: ${percent(waveformMatches, samples)})`);
console.log(`time: ${(milliseconds / voiceSeconds).toFixed(1)} ms per second of voice`);
