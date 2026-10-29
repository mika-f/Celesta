// Tests for `prepareNarration()` against a mock VOICEVOX Engine:
// pnpm --dir packages/voicevox test
//
// The mock answers `/audio_query` with the Zundamon fixture (plus a field
// Celesta does not know, which must survive into the saved query, and its
// last phrase marked as a question for upspeak) and
// `/synthesis` with a silent WAV whose length is not the query's, so the
// tests can tell a measured duration from an estimated one.

import assert from 'node:assert/strict';
import { existsSync, mkdtempSync, readFileSync, readdirSync, rmSync, writeFileSync } from 'node:fs';
import { createServer } from 'node:http';
import type { AddressInfo } from 'node:net';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { onTestFinished, test } from 'vitest';

import { NarrationError, lipSyncFromVoicevox, prepareNarration } from '../src/index.js';
import type { NarrationLine, PrepareNarrationOptions, VoicevoxVoice } from '../src/index.js';
import type { EngineQuery } from './fixtures/engine.js';

const fixture: EngineQuery = JSON.parse(
  readFileSync(new URL('./fixtures/voicevox-zundamon-konnichiwa.json', import.meta.url), 'utf8'),
);

const WAV_SECONDS = 0.5;

function silentWav(seconds = WAV_SECONDS, sampleRate = 24000): Buffer {
  const samples = Math.round(seconds * sampleRate);
  const buffer = Buffer.alloc(44 + samples * 2);
  buffer.write('RIFF', 0);
  buffer.writeUInt32LE(36 + samples * 2, 4);
  buffer.write('WAVE', 8);
  buffer.write('fmt ', 12);
  buffer.writeUInt32LE(16, 16);
  buffer.writeUInt16LE(1, 20); // PCM
  buffer.writeUInt16LE(1, 22); // mono
  buffer.writeUInt32LE(sampleRate, 24);
  buffer.writeUInt32LE(sampleRate * 2, 28);
  buffer.writeUInt16LE(2, 32);
  buffer.writeUInt16LE(16, 34);
  buffer.write('data', 36);
  buffer.writeUInt32LE(samples * 2, 40);
  return buffer;
}

/** A request the mock engine received; `query` is the JSON body, if any. */
interface EngineRequest {
  path: string;
  params: Record<string, string>;
  query: (EngineQuery & Record<string, unknown>) | undefined;
}

/** Changes the mock's answer to one request. */
interface Answer {
  status?: number;
  body?: string | Buffer;
  delay?: number;
}

type MockEngine = Awaited<ReturnType<typeof mockEngine>>;

// `behave(request)` may return { status, body, delay } to change an answer.
async function mockEngine(behave: (request: EngineRequest) => Answer | undefined = () => undefined) {
  const requests: EngineRequest[] = [];
  let inFlight = 0;
  let maxInFlight = 0;
  const server = createServer(async (req, res) => {
    const url = new URL(req.url!, 'http://localhost');
    const chunks: Buffer[] = [];
    for await (const chunk of req) chunks.push(chunk);
    const body = Buffer.concat(chunks).toString('utf8');
    const entry: EngineRequest = {
      path: url.pathname,
      params: Object.fromEntries(url.searchParams),
      query: body ? JSON.parse(body) : undefined,
    };
    if (url.pathname === '/version') {
      res.setHeader('content-type', 'application/json');
      res.end('"0.25.2"');
      return;
    }
    requests.push(entry);
    inFlight += 1;
    maxInFlight = Math.max(maxInFlight, inFlight);
    try {
      const custom: Answer = (await behave(entry)) ?? {};
      if (custom.delay) await new Promise((resolve) => setTimeout(resolve, custom.delay));
      if (res.destroyed) return;
      res.statusCode = custom.status ?? 200;
      if (custom.body !== undefined) {
        res.end(custom.body);
      } else if (url.pathname === '/audio_query') {
        res.setHeader('content-type', 'application/json');
        const query = { ...structuredClone(fixture), kana: 'コンニチワ', futureField: { keep: [1, 2] } };
        query.accent_phrases.at(-1)!.is_interrogative = true;
        res.end(JSON.stringify(query));
      } else if (url.pathname === '/synthesis') {
        res.setHeader('content-type', 'audio/wav');
        res.end(silentWav());
      } else {
        res.statusCode = 404;
        res.end();
      }
    } finally {
      inFlight -= 1;
    }
  });
  await new Promise<void>((resolve) => server.listen(0, '127.0.0.1', () => resolve()));
  // Listening on a TCP port, so the address is an AddressInfo, not a pipe name.
  const baseUrl = `http://127.0.0.1:${(server.address() as AddressInfo).port}`;
  return {
    baseUrl,
    requests,
    get maxInFlight() {
      return maxInFlight;
    },
    syntheses: () => requests.filter((request) => request.path === '/synthesis'),
    reset() {
      requests.length = 0;
      maxInFlight = 0;
    },
    close: () =>
      new Promise<void>((resolve) => {
        server.closeAllConnections();
        server.close(() => resolve());
      }),
  };
}

function setup() {
  const dir = mkdtempSync(join(tmpdir(), 'celesta-narration-'));
  onTestFinished(() => rmSync(dir, { recursive: true, force: true }));
  return join(dir, 'voices');
}

const script: NarrationLine[] = [
  { id: 'intro', speaker: 'zunda', text: 'HPが1です。', spokenText: 'エイチピーがいちです。', expression: 'smile' },
  { id: 'reply', speaker: 'metan', text: '危ないわね。', gap: 0.5 },
];

const speakersFor = (
  engine: Pick<MockEngine, 'baseUrl'>,
  extra: { zunda?: Partial<VoicevoxVoice>; metan?: Partial<VoicevoxVoice> } = {},
): Record<string, VoicevoxVoice> => ({
  zunda: { baseUrl: engine.baseUrl, styleId: 3, ...extra.zunda },
  metan: { baseUrl: `${engine.baseUrl}/`, styleId: 2, query: { speedScale: 1.25, pauseLengthScale: 2 }, ...extra.metan },
});

test('synthesizes a two-speaker script into lines for planDialogue()', async () => {
  const engine = await mockEngine();
  onTestFinished(engine.close);
  const cacheDir = setup();

  const lines = await prepareNarration(script, { speakers: speakersFor(engine), cacheDir });

  const queries = engine.requests.filter((request) => request.path === '/audio_query');
  assert.deepEqual(
    queries.map((request) => request.params).sort((a, b) => a.speaker.localeCompare(b.speaker)),
    [
      { text: '危ないわね。', speaker: '2' },
      { text: 'エイチピーがいちです。', speaker: '3' },
    ],
  );
  const syntheses = engine.syntheses().sort((a, b) => a.params.speaker.localeCompare(b.params.speaker));
  assert.deepEqual(syntheses.map((request) => request.params), [
    { speaker: '2', enable_interrogative_upspeak: 'true' },
    { speaker: '3', enable_interrogative_upspeak: 'true' },
  ]);
  // The voice's settings are applied over the engine's query; the rest survives.
  assert.equal(syntheses[0].query!.speedScale, 1.25);
  assert.equal(syntheses[0].query!.pauseLengthScale, 2);
  assert.equal(syntheses[1].query!.speedScale, fixture.speedScale);
  assert.deepEqual(syntheses[0].query!.futureField, { keep: [1, 2] });

  assert.equal(lines.length, 2);
  const [intro, reply] = lines;
  // Layout and presentation fields are passed through.
  assert.equal(intro.expression, 'smile');
  assert.equal(intro.text, 'HPが1です。');
  assert.equal(reply.gap, 0.5);
  for (const line of lines) {
    assert.ok(existsSync(line.audio));
    assert.equal(line.audio, line.narration!.audio);
    assert.equal(line.narration!.cached, false);
    // Measured from the WAV, not estimated from the query.
    assert.equal(line.narration!.duration, WAV_SECONDS);
    const saved = JSON.parse(readFileSync(line.narration!.metadata, 'utf8'));
    assert.deepEqual(saved.query, line.narration!.query);
    assert.equal(saved.query.kana, 'コンニチワ');
    assert.deepEqual(saved.query.futureField, { keep: [1, 2] });
    assert.equal(saved.engine.version, '0.25.2');
  }
  assert.equal(intro.narration!.spokenText, 'エイチピーがいちです。');
  assert.equal(reply.narration!.query.speedScale, 1.25);
  // The lip sync comes from the saved query, with the synthesis flag.
  const expected = lipSyncFromVoicevox(reply.narration!.query, { interrogativeUpspeak: true });
  const unscaled = lipSyncFromVoicevox({ ...reply.narration!.query, speedScale: 1, pauseLengthScale: 1 });
  const flat = lipSyncFromVoicevox(reply.narration!.query, { interrogativeUpspeak: false });
  assert.equal(reply.lipSync!.durationInSeconds, expected.durationInSeconds);
  assert.notEqual(reply.lipSync!.durationInSeconds, unscaled.durationInSeconds);
  assert.ok(reply.lipSync!.durationInSeconds > flat.durationInSeconds, 'upspeak adds a mora');
  for (let at = 0; at < expected.durationInSeconds; at += 0.05) {
    assert.equal(reply.lipSync!.mouthAtSeconds(at), expected.mouthAtSeconds(at));
  }
});

test('an unchanged script is read from the cache without the engine', async () => {
  const engine = await mockEngine();
  const cacheDir = setup();
  const speakers = speakersFor(engine);
  const first = await prepareNarration(script, { speakers, cacheDir });
  await engine.close();

  const second = await prepareNarration(script, { speakers, cacheDir });
  assert.deepEqual(second.map((line) => line.narration!.cached), [true, true]);
  assert.deepEqual(second.map((line) => line.audio), first.map((line) => line.audio));
  assert.deepEqual(second.map((line) => line.narration!.query), first.map((line) => line.narration!.query));
  assert.equal(second[1].lipSync!.durationInSeconds, first[1].lipSync!.durationInSeconds);
});

test('only lines whose spoken input changed are synthesized again', async () => {
  const engine = await mockEngine();
  onTestFinished(engine.close);
  const cacheDir = setup();
  const speakers = speakersFor(engine);
  await prepareNarration(script, { speakers, cacheDir });

  // Display text with the same spokenText, expression, order, gap, scene: no audio.
  engine.reset();
  const restyled = [
    { ...script[1], gap: 1, scene: 'b', expression: 'surprised' },
    { ...script[0], text: 'HP が 1 です!', expression: 'angry', leadIn: 0.3 },
  ];
  const reused = await prepareNarration(restyled, { speakers, cacheDir });
  assert.equal(engine.requests.length, 0);
  assert.deepEqual(reused.map((line) => line.id), ['reply', 'intro']);
  assert.deepEqual(reused.map((line) => line.narration!.cached), [true, true]);

  // One spoken line changes: one query and one synthesis.
  engine.reset();
  const edited = [script[0], { ...script[1], text: '危ないわよ。' }];
  const lines = await prepareNarration(edited, { speakers, cacheDir });
  assert.deepEqual(engine.requests.map((request) => request.path), ['/audio_query', '/synthesis']);
  assert.equal(engine.requests[0].params.text, '危ないわよ。');
  assert.deepEqual(lines.map((line) => line.narration!.cached), [true, false]);
});

test('style, settings, and revisions invalidate only the lines they affect', async () => {
  const engine = await mockEngine();
  onTestFinished(engine.close);
  const cacheDir = setup();
  await prepareNarration(script, { speakers: speakersFor(engine), cacheDir });

  const missesWith = async (
    speakers: Record<string, VoicevoxVoice>,
    options: Omit<PrepareNarrationOptions, 'speakers' | 'cacheDir'> = {},
    lines: readonly NarrationLine[] = script,
  ) => {
    engine.reset();
    const result = await prepareNarration(lines, { speakers, cacheDir, ...options });
    return result.filter((line) => !line.narration!.cached).map((line) => line.id);
  };
  assert.deepEqual(await missesWith(speakersFor(engine, { zunda: { styleId: 1 } })), ['intro']);
  assert.deepEqual(await missesWith(speakersFor(engine, { metan: { query: { speedScale: 1.1 } } })), ['reply']);
  assert.deepEqual(await missesWith(speakersFor(engine, { metan: { revision: 'dict-2' } })), ['reply']);
  assert.deepEqual(await missesWith(speakersFor(engine, { zunda: { interrogativeUpspeak: false } })), ['intro']);
  assert.deepEqual(await missesWith(speakersFor(engine), { revision: 'engine-0.26' }), ['intro', 'reply']);
  // A per-line override is part of only that line's key.
  assert.deepEqual(
    await missesWith(speakersFor(engine), {}, [script[0], { ...script[1], voice: { query: { pitchScale: 0.05 } } }]),
    ['reply'],
  );
  assert.equal(engine.syntheses()[0].query!.pitchScale, 0.05);
  assert.equal(engine.syntheses()[0].query!.speedScale, 1.25);
  // The engine address is not: another port reads the same entries.
  engine.reset();
  const moved = await prepareNarration(script, {
    speakers: speakersFor({ baseUrl: 'http://127.0.0.1:9' }),
    cacheDir,
  });
  assert.deepEqual(moved.map((line) => line.narration!.cached), [true, true]);
  // `refresh` redoes the named lines even though their entries are valid.
  assert.deepEqual(await missesWith(speakersFor(engine), { refresh: ['reply'] }), ['reply']);
  assert.deepEqual(await missesWith(speakersFor(engine), { refresh: true }), ['intro', 'reply']);
});

test('identical lines share one synthesis and requests stay within the concurrency', async () => {
  const engine = await mockEngine(() => ({ delay: 30 }));
  onTestFinished(engine.close);
  const cacheDir = setup();
  const lines = [
    { id: 'a', speaker: 'zunda', text: 'いち' },
    { id: 'b', speaker: 'zunda', text: 'いち' },
    { id: 'c', speaker: 'zunda', text: 'に' },
    { id: 'd', speaker: 'zunda', text: 'さん' },
    { id: 'e', speaker: 'zunda', text: 'し' },
  ];
  const result = await prepareNarration(lines, { speakers: speakersFor(engine), cacheDir, concurrency: 2 });
  assert.equal(engine.syntheses().length, 4);
  assert.equal(result[0].audio, result[1].audio);
  assert.ok(engine.maxInFlight <= 2, `max in flight ${engine.maxInFlight}`);
  assert.equal(readdirSync(cacheDir).filter((name) => name.endsWith('.wav')).length, 4);
});

test('a failed line names itself, and a rerun redoes only it', async () => {
  let broken = true;
  const engine = await mockEngine((request) =>
    broken && request.path === '/synthesis' && request.params.speaker === '2'
      ? { status: 500, body: 'model crashed' }
      : undefined,
  );
  onTestFinished(engine.close);
  const cacheDir = setup();
  const speakers = speakersFor(engine);
  await assert.rejects(prepareNarration(script, { speakers, cacheDir }), (error) => {
    assert.ok(error instanceof NarrationError);
    assert.equal(error.lineId, 'reply');
    assert.equal(error.speaker, 'metan');
    assert.equal(error.stage, 'synthesis');
    assert.match(error.message, /line "reply" \(speaker "metan"\) failed at synthesis: .*500: model crashed/);
    return true;
  });
  // The good line is cached; the failed one left nothing behind.
  assert.equal(readdirSync(cacheDir).filter((name) => name.endsWith('.json')).length, 1);
  assert.equal(readdirSync(cacheDir).filter((name) => name.endsWith('.tmp')).length, 0);

  broken = false;
  engine.reset();
  const lines = await prepareNarration(script, { speakers, cacheDir });
  assert.deepEqual(lines.map((line) => line.narration!.cached), [true, false]);
  assert.equal(engine.syntheses().length, 1);
});

test('cancellation and timeouts reject without leaving an entry', async () => {
  const engine = await mockEngine((request) => (request.path === '/synthesis' ? { delay: 2000 } : undefined));
  onTestFinished(engine.close);
  const cacheDir = setup();
  const speakers = speakersFor(engine);

  const controller = new AbortController();
  const pending = prepareNarration(script.slice(0, 1), { speakers, cacheDir, signal: controller.signal });
  setTimeout(() => controller.abort(), 100);
  await assert.rejects(pending, (error) => {
    assert.ok(error instanceof NarrationError);
    assert.equal(error.lineId, 'intro');
    assert.equal(error.stage, 'synthesis');
    assert.match(error.message, /cancelled/);
    return true;
  });
  await assert.rejects(
    prepareNarration(script.slice(0, 1), { speakers, cacheDir, signal: AbortSignal.abort() }),
    /failed at audio_query: cancelled/,
  );
  await assert.rejects(
    prepareNarration(script.slice(0, 1), { speakers, cacheDir, timeout: 100 }),
    (error) => error instanceof NarrationError && error.stage === 'synthesis' && /did not answer within 100 ms/.test(error.message),
  );
  assert.ok(!existsSync(cacheDir) || readdirSync(cacheDir).length === 0);
});

test('configuration errors fail before any request', async () => {
  const engine = await mockEngine();
  onTestFinished(engine.close);
  const cacheDir = setup();
  const speakers = speakersFor(engine);
  await assert.rejects(
    prepareNarration([script[0], { id: 'x', speaker: 'nobody', text: 'え' }], { speakers, cacheDir }),
    (error) =>
      error instanceof NarrationError &&
      error.lineId === 'x' &&
      error.stage === 'config' &&
      /not in `speakers` \(known: "zunda", "metan"\)/.test(error.message),
  );
  await assert.rejects(
    prepareNarration([{ id: 'x', speaker: 'zunda', text: ['a', 'b'] }], { speakers, cacheDir }),
    /give the line a `spokenText`/,
  );
  await assert.rejects(
    prepareNarration([script[0], { ...script[0] }], { speakers, cacheDir }),
    /line "intro" \(speaker "zunda"\) failed at config: the id is used more than once/,
  );
  await assert.rejects(
    // Wrong on purpose: a voice without its engine, as untyped config may have.
    prepareNarration(script, { speakers: { ...speakers, zunda: { styleId: 3 } as VoicevoxVoice }, cacheDir }),
    /needs a `baseUrl`/,
  );
  assert.equal(engine.requests.length, 0);
});

test('an unavailable engine and malformed answers are reported by stage', async () => {
  const cacheDir = setup();
  const closed = await mockEngine();
  await closed.close();
  await assert.rejects(
    prepareNarration(script.slice(0, 1), { speakers: speakersFor(closed), cacheDir }),
    (error) => error instanceof NarrationError && error.stage === 'audio_query' && /could not reach the VOICEVOX Engine at http:\/\/127\.0\.0\.1:\d+/.test(error.message),
  );

  const notJson = await mockEngine((request) => (request.path === '/audio_query' ? { body: '<html>' } : undefined));
  onTestFinished(notJson.close);
  await assert.rejects(
    prepareNarration(script.slice(0, 1), { speakers: speakersFor(notJson), cacheDir }),
    /failed at audio_query: the engine returned a query that is not JSON/,
  );
  const noPhrases = await mockEngine((request) => (request.path === '/audio_query' ? { body: '{"speedScale":1}' } : undefined));
  onTestFinished(noPhrases.close);
  await assert.rejects(
    prepareNarration(script.slice(0, 1), { speakers: speakersFor(noPhrases), cacheDir }),
    /without `accent_phrases`/,
  );
  const badWav = await mockEngine((request) => (request.path === '/synthesis' ? { body: 'RIFF....nope' } : undefined));
  onTestFinished(badWav.close);
  await assert.rejects(
    prepareNarration(script.slice(0, 1), { speakers: speakersFor(badWav), cacheDir }),
    /failed at synthesis: the engine returned audio that is not a readable WAV/,
  );
  const emptyWav = await mockEngine((request) => (request.path === '/synthesis' ? { body: silentWav(0) } : undefined));
  onTestFinished(emptyWav.close);
  await assert.rejects(
    prepareNarration(script.slice(0, 1), { speakers: speakersFor(emptyWav), cacheDir }),
    /failed at synthesis: the engine returned audio that is not a readable WAV \(it has no samples\)/,
  );
  assert.ok(!existsSync(cacheDir) || readdirSync(cacheDir).length === 0);
});

test('missing or corrupt entries are synthesized again', async () => {
  const engine = await mockEngine();
  onTestFinished(engine.close);
  const cacheDir = setup();
  const speakers = speakersFor(engine);
  const [intro, reply] = await prepareNarration(script, { speakers, cacheDir });

  writeFileSync(intro.audio, readFileSync(intro.audio).subarray(0, 100)); // truncated WAV
  rmSync(reply.narration!.metadata); // lost metadata
  engine.reset();
  const lines = await prepareNarration(script, { speakers, cacheDir });
  assert.deepEqual(lines.map((line) => line.narration!.cached), [false, false]);
  assert.equal(engine.syntheses().length, 2);
  assert.equal(lines[0].narration!.duration, WAV_SECONDS);

  writeFileSync(lines[1].narration!.metadata, '{ not json');
  engine.reset();
  const again = await prepareNarration(script, { speakers, cacheDir });
  assert.deepEqual(again.map((line) => line.narration!.cached), [true, false]);
});

test('a cache directory that cannot be written fails at write', async () => {
  const engine = await mockEngine();
  onTestFinished(engine.close);
  const cacheDir = setup();
  writeFileSync(join(cacheDir, '..', 'blocked'), 'a file, not a directory');
  await assert.rejects(
    prepareNarration(script.slice(0, 1), { speakers: speakersFor(engine), cacheDir: join(cacheDir, '..', 'blocked') }),
    (error) => error instanceof NarrationError && error.stage === 'write' && /could not write the cache entry/.test(error.message),
  );
});

test('lines with their own audio are passed through without synthesis', async () => {
  const engine = await mockEngine();
  onTestFinished(engine.close);
  const cacheDir = setup();
  const handWritten = { id: 'old', text: '手書きの台詞', audio: './voices/old.wav', durationInFrames: 30 };
  const lines = await prepareNarration([handWritten, script[0]], { speakers: speakersFor(engine), cacheDir });
  assert.equal(lines[0], handWritten);
  assert.equal(lines[0].narration, undefined);
  assert.equal(engine.syntheses().length, 1);
});

test('a relative cacheDir resolves from the entry directory', async () => {
  const engine = await mockEngine();
  onTestFinished(engine.close);
  const cacheDir = setup();
  const previous = process.env.CELESTA_REACT_ENTRY_DIR;
  process.env.CELESTA_REACT_ENTRY_DIR = join(cacheDir, '..');
  onTestFinished(() => {
    if (previous === undefined) delete process.env.CELESTA_REACT_ENTRY_DIR;
    else process.env.CELESTA_REACT_ENTRY_DIR = previous;
  });
  const [line] = await prepareNarration(script.slice(0, 1), { speakers: speakersFor(engine) });
  assert.ok(line.audio.startsWith(join(cacheDir, '..', '.celesta', 'voices')));
});
