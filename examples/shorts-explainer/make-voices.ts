// Reads every line of script.ts aloud with VOICEVOX Engine and writes, per
// line, voices/<id>.json (the AudioQuery used for synthesis) and
// voices/<id>.wav. film.tsx times every line from these WAVs and builds the
// lip sync from the queries, so after editing the script, run this again and
// the video follows.
//
//   docker run --rm -p 50021:50021 voicevox/voicevox_engine:cpu-ubuntu24.04-latest
//   node examples/shorts-explainer/make-voices.ts
//
// Needs Node.js 23.6 or later (it runs this TypeScript file directly). Set
// VOICEVOX_URL to use an engine somewhere other than http://127.0.0.1:50021.

import { mkdir, readdir, rm, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { SCRIPT, SPEAKERS, plainText } from './script.ts';

const ENGINE = (process.env.VOICEVOX_URL ?? 'http://127.0.0.1:50021').replace(/\/$/, '');
const OUT = join(import.meta.dirname, 'voices');

async function call(path: string, init?: RequestInit): Promise<Response> {
  let response: Response;
  try {
    response = await fetch(`${ENGINE}${path}`, { method: 'POST', ...init });
  } catch (error) {
    throw new Error(`cannot reach VOICEVOX Engine at ${ENGINE} (start it with docker, see the top of this file): ${error}`);
  }
  if (!response.ok) {
    throw new Error(`${path.split('?')[0]} failed: ${response.status} ${await response.text()}`);
  }
  return response;
}

// A WAV's length from its header: the `data` chunk's size over the byte rate
// in `fmt `, so stereo or another sampling rate is measured correctly too.
function wavSeconds(bytes: Uint8Array): number {
  const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  const tag = (at: number) => String.fromCharCode(...bytes.subarray(at, at + 4));
  let byteRate = 0;
  for (let at = 12; at + 8 <= bytes.length;) {
    const size = view.getUint32(at + 4, true);
    if (tag(at) === 'fmt ') byteRate = view.getUint32(at + 16, true);
    if (tag(at) === 'data' && byteRate > 0) return size / byteRate;
    at += 8 + size + (size % 2);
  }
  throw new Error('VOICEVOX returned a WAV without fmt / data chunks');
}

async function main() {
  const version = await (await call('/version', { method: 'GET' })).json();
  console.log(`VOICEVOX Engine ${version} at ${ENGINE}`);
  await mkdir(OUT, { recursive: true });

  let seconds = 0;
  for (const line of SCRIPT) {
    const speaker = SPEAKERS[line.speaker];
    const text = line.reading ?? plainText(line.text);
    const params = new URLSearchParams({ speaker: String(speaker.style), text });
    const query = await (await call(`/audio_query?${params}`)).json();
    // A short's tempo: a little faster than the engine's default, with
    // short silences at both ends (the plan adds the gaps between lines).
    query.speedScale = line.speed ?? speaker.speed;
    query.pitchScale = speaker.pitch ?? 0;
    query.intonationScale = speaker.intonation ?? 1.1;
    query.prePhonemeLength = 0.05;
    query.postPhonemeLength = 0.08;
    const wav = await call(`/synthesis?speaker=${speaker.style}`, {
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(query),
    });
    const bytes = new Uint8Array(await wav.arrayBuffer());
    await writeFile(join(OUT, `${line.id}.json`), `${JSON.stringify(query, null, 2)}\n`);
    await writeFile(join(OUT, `${line.id}.wav`), bytes);
    const length = wavSeconds(bytes);
    seconds += length;
    console.log(`${line.id.padEnd(12)} ${length.toFixed(2).padStart(5)} s  ${speaker.name}: ${text}`);
  }

  // Remove voices of lines that are no longer in the script.
  const keep = new Set(SCRIPT.flatMap((l) => [`${l.id}.json`, `${l.id}.wav`]));
  for (const file of await readdir(OUT)) {
    if (/\.(json|wav)$/.test(file) && !keep.has(file)) {
      await rm(join(OUT, file));
      console.log(`removed ${file}`);
    }
  }
  console.log(`${SCRIPT.length} lines, ${seconds.toFixed(1)} s of speech`);
}

main().catch((error) => {
  console.error(error instanceof Error ? error.message : error);
  process.exit(1);
});
