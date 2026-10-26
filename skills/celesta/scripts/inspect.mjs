#!/usr/bin/env node
// Checks a Celesta React composition without the GUI: loads the entry with
// Celesta's own runtime (the same bundler, prepare() call, and renderer
// protocol the app and exporter use), evaluates a few frames, and prints
// every layer, audio clip, and missing media file. Also lists PSD layer
// paths for portrait setup.
//
//   node inspect.mjs <entry.tsx> [--frames 0,45,-1] [--every N] [--json]
//                    [--props-file variant.json] [--props '{"title":"..."}']
//   node inspect.mjs <entry.tsx> --loop [--json]
//   node inspect.mjs --psd-layers <file.psd>
//
// --loop checks a seamless loop: it evaluates frame N (one past the last) as
// if the video went on, and compares its Scene with frame 0's (and N + 1
// with 1). A <Sequence> that lasts until the end stays open at N, and
// useVideoConfig() still reports the real duration.
// --props/--props-file pass project property values exactly as
// Celesta-export does (--props wins; relative paths in the file resolve from
// the file's directory).
// Options: --runtime <cli.js> and --node <node> override runtime discovery
// (also CELESTA_REACT_CLI / CELESTA_NODE). --timeout <seconds> (default 180).
// Frames are evaluated through Celesta-export --inspect when it is found (a
// source checkout's build above the entry or the current directory, then the
// default macOS and Windows install locations), with real text shaping
// (measureText(), useTextMetrics(), <TextBox>, @celesta/code, …) and media
// probing, still without rendering pixels. --native <Celesta-export> picks the
// exporter; --no-native evaluates in Node only, which cannot shape text.
// Exit code 1 means the entry failed to load, a frame failed, or a
// referenced local file is missing. Exit code 2 means only that text had to
// be measured, which needs the exporter or a PNG export.

import { spawn, spawnSync } from 'node:child_process';
import { existsSync, mkdtempSync, readFileSync, rmSync, statSync, writeFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import { homedir, tmpdir } from 'node:os';
import * as path from 'node:path';
import * as readline from 'node:readline';

const USAGE = `usage: node inspect.mjs <entry.tsx> [--frames 0,45,-1] [--every N] [--json]
                         [--composition <id>] [--props-file <file.json>] [--props <json>]
       node inspect.mjs <entry.tsx> --loop [--json]
       node inspect.mjs --psd-layers <file.psd>
options: --native <Celesta-export>  --no-native  --runtime <cli.js>  --node <node>  --timeout <seconds>`;

// Answers measureText requests in Node-only mode; an error carrying it is an
// inspector limitation, not a scene error.
const TEXT_UNSUPPORTED =
  'inspect.mjs cannot shape text, so measureText(), useTextMetrics(), fitText(), useFitText(), <TextBox>, ' +
  '@celesta/code, and subtitle.render metrics fail here; inspect with --native <Celesta-export>, ' +
  'or check this entry with a PNG export (Celesta-export --react <entry> --frame N out.png) instead';

function parseArgs(argv) {
  const options = {
    frames: null, every: null, loop: false, json: false, psd: null, native: null, runtime: null, node: null, timeout: 180, entry: null,
    props: null, propsFile: null, composition: null,
  };
  for (let i = 0; i < argv.length; i += 1) {
    const arg = argv[i];
    const value = () => {
      if (i + 1 >= argv.length) fail(`${arg} needs a value\n${USAGE}`);
      return argv[++i];
    };
    if (arg === '--frames') options.frames = value().split(',').map((part) => parseInt(part.trim(), 10));
    else if (arg === '--every') options.every = parseInt(value(), 10);
    else if (arg === '--loop') options.loop = true;
    else if (arg === '--json') options.json = true;
    else if (arg === '--psd-layers') options.psd = value();
    else if (arg === '--runtime') options.runtime = value();
    else if (arg === '--native') options.native = value();
    else if (arg === '--no-native') options.native = false;
    else if (arg === '--node') options.node = value();
    else if (arg === '--timeout') options.timeout = Number(value());
    else if (arg === '--composition') options.composition = value();
    else if (arg === '--props') options.props = value();
    else if (arg === '--props-file') options.propsFile = value();
    else if (arg === '-h' || arg === '--help') { console.log(USAGE); process.exit(0); }
    else if (arg.startsWith('--')) fail(`unknown option ${arg}\n${USAGE}`);
    else options.entry = arg;
  }
  if (options.frames?.some(Number.isNaN)) fail('--frames takes comma-separated integers (negative counts from the end)');
  if (options.every !== null && !(options.every > 0)) fail('--every takes a positive integer');
  if (options.loop && (options.frames || options.every !== null)) fail('--loop picks its own frames; leave out --frames and --every');
  if (!options.entry && !options.psd) fail(USAGE);
  return options;
}

function fail(message) {
  console.error(message);
  process.exit(1);
}

// ---------------------------------------------------------------- runtime

function findRuntime(options, startDirs) {
  const explicit = options.runtime ?? process.env.CELESTA_REACT_CLI;
  const candidates = [];
  if (explicit) candidates.push(path.resolve(explicit));
  // A source checkout: <repo>/packages/cli/dist/cli.js above the entry or cwd.
  for (const start of startDirs) {
    for (let dir = path.resolve(start); ; dir = path.dirname(dir)) {
      candidates.push(path.join(dir, 'packages/cli/dist/cli.js'));
      if (path.dirname(dir) === dir) break;
    }
  }
  if (process.platform === 'darwin') {
    for (const apps of ['/Applications', path.join(homedir(), 'Applications')]) {
      candidates.push(path.join(apps, 'Celesta.app/Contents/Resources/react/dist/cli.js'));
    }
  }
  if (process.platform === 'win32' && process.env.LOCALAPPDATA) {
    candidates.push(path.join(process.env.LOCALAPPDATA, 'Programs/Celesta/runtime/react/dist/cli.js'));
  }
  const cli = candidates.find((candidate) => existsSync(candidate));
  if (!cli) {
    fail(
      'Could not find the Celesta React runtime (react/dist/cli.js).\n' +
        'Pass --runtime <path to cli.js>, for example:\n' +
        '  macOS:   /Applications/Celesta.app/Contents/Resources/react/dist/cli.js\n' +
        '  Windows: <Celesta folder>\\runtime\\react\\dist\\cli.js\n' +
        '  source:  <repo>/packages/cli/dist/cli.js (run pnpm install, codegen, build:runtime first)',
    );
  }
  return { cli, node: findNode(options, cli) };
}

function findNode(options, cli) {
  const explicit = options.node ?? process.env.CELESTA_NODE;
  if (explicit) return explicit;
  // Prefer the Node.js bundled with a packaged app, next to its runtime.
  const reactDir = path.dirname(path.dirname(cli)); // …/react
  const bundled = [
    path.join(reactDir, '../../Helpers/node'), // macOS: Contents/Resources/react → Contents/Helpers/node
    path.join(reactDir, '../node.exe'), // Windows: runtime/react → runtime/node.exe
    path.join(reactDir, '../node'),
  ].find((candidate) => existsSync(candidate));
  return bundled ?? process.execPath;
}

// --------------------------------------------------------------- exporter

/** The first exporter that supports --inspect: a source build, then an install. */
function findExporter(startDirs, loop) {
  const exe = process.platform === 'win32' ? '.exe' : '';
  const candidates = [];
  for (const start of startDirs) {
    for (let dir = path.resolve(start); ; dir = path.dirname(dir)) {
      // A source checkout: its newer build first.
      candidates.push(
        ...['release', 'debug']
          .map((profile) => path.join(dir, 'target', profile, `celesta-exporter${exe}`))
          .filter((candidate) => existsSync(candidate))
          .sort((a, b) => statSync(b).mtimeMs - statSync(a).mtimeMs),
      );
      if (path.dirname(dir) === dir) break;
    }
  }
  if (process.platform === 'darwin') {
    for (const apps of ['/Applications', path.join(homedir(), 'Applications')]) {
      candidates.push(path.join(apps, 'Celesta.app/Contents/MacOS/Celesta-export'));
    }
  }
  if (process.platform === 'win32' && process.env.LOCALAPPDATA) {
    candidates.push(path.join(process.env.LOCALAPPDATA, 'Programs/Celesta/Celesta-export.exe'));
  }
  // Exporters from before --inspect are skipped, and for --loop those from
  // before openEnd, which Node-only inspection handles.
  const supportsInspect = (exporter) => {
    const help = spawnSync(exporter, ['--help'], { encoding: 'utf8', timeout: 10000 });
    return help.status === 0 && help.stdout.includes('--inspect') && (!loop || help.stdout.includes('openEnd'));
  };
  return candidates.find((candidate) => existsSync(candidate) && supportsInspect(candidate)) ?? false;
}

// ------------------------------------------------------------ media probe

let ffprobeAvailable;
function probeMedia(mediaPath) {
  ffprobeAvailable ??= spawnSync('ffprobe', ['-version'], { stdio: 'ignore' }).status === 0;
  if (!ffprobeAvailable) {
    return { error: 'inspect.mjs answers preloadMedia() with ffprobe, which is not installed; install FFmpeg or give prepare() a fallback' };
  }
  const result = spawnSync('ffprobe', ['-v', 'error', '-print_format', 'json', '-show_format', '-show_streams', mediaPath], { encoding: 'utf8' });
  if (result.status !== 0) return { error: `ffprobe failed for ${mediaPath}: ${result.stderr.trim()}` };
  const data = JSON.parse(result.stdout);
  const seconds = (value) => (value === undefined || Number.isNaN(Number(value)) ? undefined : Number(value));
  const video = data.streams.find((stream) => stream.codec_type === 'video' && !stream.disposition?.attached_pic);
  let frameRate;
  if (video?.avg_frame_rate && video.avg_frame_rate !== '0/0') {
    const [numerator, denominator] = video.avg_frame_rate.split('/').map(Number);
    frameRate = { numerator, denominator };
  }
  // Celesta's probe omits what FFmpeg reports as unknown, and the whole
  // object when nothing is tagged.
  const tag = (value) => (value === undefined || value === 'unknown' || value === 'reserved' ? undefined : value);
  const color = video && { primaries: tag(video.color_primaries), transfer: tag(video.color_transfer), matrix: tag(video.color_space) };
  const hdr = { smpte2084: 'pq', 'arib-std-b67': 'hlg' }[color?.transfer];
  return {
    media: {
      durationSeconds: seconds(data.format?.duration),
      video: video && {
        codec: video.codec_name,
        width: video.width,
        height: video.height,
        frameRate,
        durationSeconds: seconds(video.duration),
        color: Object.values(color).some((value) => value !== undefined) ? color : undefined,
        hdr,
      },
      audio: data.streams
        .filter((stream) => stream.codec_type === 'audio')
        .map((stream) => ({ codec: stream.codec_name, sampleRate: Number(stream.sample_rate), channels: stream.channels, durationSeconds: seconds(stream.duration) })),
    },
  };
}

// Holds the decoded file handed to the runtime last, removed when its next
// message arrives: it has read the file by then, as with Celesta's own answer.
let decodedAudioDirectory;
let ffmpegAvailable;
function decodeAudio(mediaPath) {
  ffmpegAvailable ??= spawnSync('ffmpeg', ['-version'], { stdio: 'ignore' }).status === 0;
  if (!ffmpegAvailable) {
    return { error: 'inspect.mjs answers loadAudioSpectrum() with ffmpeg, which is not installed; install FFmpeg or use --native' };
  }
  const probed = probeMedia(mediaPath);
  if (probed.error) return probed;
  const stream = probed.media.audio[0];
  if (!stream) return { error: `could not decode ${mediaPath}: it has no audio stream` };
  // Like Celesta: the stream's own rate, mono kept, wider mixed to stereo,
  // then the channels averaged.
  const sampleRate = stream.sampleRate || 48000;
  const channels = stream.channels === 1 ? 1 : 2;
  const result = spawnSync('ffmpeg', [
    '-v', 'error', '-i', mediaPath, '-map', '0:a:0', '-ac', String(channels), '-ar', String(sampleRate),
    '-f', 'f32le', '-acodec', 'pcm_f32le', '-',
  ], { maxBuffer: 2 ** 31 });
  if (result.status !== 0) return { error: `could not decode ${mediaPath}: ${result.stderr.toString().trim()}` };
  const frames = Math.floor(result.stdout.length / 4 / channels);
  const mono = Buffer.alloc(frames * 4);
  for (let frame = 0; frame < frames; frame += 1) {
    let sum = 0;
    for (let channel = 0; channel < channels; channel += 1) sum += result.stdout.readFloatLE((frame * channels + channel) * 4);
    mono.writeFloatLE(sum / channels, frame * 4);
  }
  // A new private directory, so nothing another user placed in a shared
  // temporary directory is written through.
  decodedAudioDirectory = mkdtempSync(path.join(tmpdir(), 'celesta-inspect-'));
  const file = path.join(decodedAudioDirectory, 'audio.f32le');
  writeFileSync(file, mono, { flag: 'wx', mode: 0o600 });
  return { audio: { path: file, sampleRate, frames } };
}

function removeDecodedAudio() {
  if (decodedAudioDirectory) rmSync(decodedAudioDirectory, { recursive: true, force: true });
  decodedAudioDirectory = undefined;
}

// ------------------------------------------------------------ inspection

/** The same layers Celesta-export sends: --props-file or the entry's props.json, then --props on top. */
function readPropertyInputs(options, entry) {
  const layers = [];
  const object = (text, origin) => {
    let value;
    try {
      value = JSON.parse(text);
    } catch (error) {
      fail(`${origin} is not valid JSON: ${error.message}`);
    }
    if (value === null || typeof value !== 'object' || Array.isArray(value)) {
      fail(`${origin} must be a JSON object of property values`);
    }
    return value;
  };
  if (options.propsFile !== null) {
    const file = path.resolve(options.propsFile);
    if (!existsSync(file)) fail(`--props-file not found: ${file}`);
    layers.push({
      source: 'propsFile',
      file: options.propsFile,
      baseDir: path.dirname(file),
      values: object(readFileSync(file, 'utf8'), `--props-file ${options.propsFile}`),
    });
  }
  if (options.propsFile === null) {
    const baseDir = path.dirname(entry);
    const file = path.join(baseDir, 'props.json');
    if (existsSync(file) && statSync(file).isFile()) {
      layers.push({
        source: 'entryProps',
        file,
        baseDir,
        values: object(readFileSync(file, 'utf8'), `props.json ${file}`),
      });
    }
  }
  if (options.props !== null) {
    layers.push({ source: 'props', baseDir: process.cwd(), values: object(options.props, '--props') });
  }
  return layers;
}

async function inspectEntry(options) {
  const entry = path.resolve(options.entry);
  if (!existsSync(entry)) fail(`entry not found: ${entry}`);
  const entryDir = path.dirname(entry);
  const found = options.native === null;
  if (found) options.native = findExporter([entryDir, process.cwd()], options.loop);
  let child;
  if (options.native) {
    // The exporter finds its own runtime and reads the props itself.
    const args = ['--inspect', '--react', entry];
    const cli = options.runtime ?? process.env.CELESTA_REACT_CLI;
    const node = options.node ?? process.env.CELESTA_NODE;
    if (cli) args.push('--runtime', cli);
    if (node) args.push('--node', node);
    if (options.propsFile !== null) args.push('--props-file', options.propsFile);
    if (options.props !== null) args.push('--props', options.props);
    if (options.composition !== null) args.push('--composition', options.composition);
    if (!options.json) console.log(`native:  ${options.native}${found ? ' (found)' : ''}\nentry:   ${entry}\n`);
    // Its own process group, so a timeout also ends the Node runtime it starts.
    child = spawn(options.native, args, { stdio: ['pipe', 'pipe', 'inherit'], detached: process.platform !== 'win32' });
    if (process.platform !== 'win32') {
      // Terminal signals reach only the foreground group; pass them on.
      for (const signal of ['SIGINT', 'SIGTERM', 'SIGHUP']) {
        process.once(signal, () => {
          try { process.kill(-child.pid, signal); } catch {}
          process.kill(process.pid, signal);
        });
      }
    }
  } else {
    const { cli, node } = findRuntime(options, [entryDir, process.cwd()]);
    if (!options.json) console.log(`runtime: ${cli}\nnode:    ${node}\nentry:   ${entry}\n`);
    const propertyInputs = readPropertyInputs(options, entry);
    const args = propertyInputs.length > 0 ? [cli, entry, '--properties-stdin'] : [cli, entry];
    if (options.composition !== null) args.push('--composition', options.composition);
    child = spawn(node, args, { stdio: ['pipe', 'pipe', 'inherit'] });
    if (propertyInputs.length > 0) child.stdin.write(`${JSON.stringify({ propertyInputs })}\n`);
  }
  const exited = new Promise((resolve) => child.once('close', resolve));
  const stop = () => {
    if (!options.native || !child.pid) child.kill();
    else if (process.platform === 'win32') spawnSync('taskkill', ['/pid', String(child.pid), '/t', '/f'], { stdio: 'ignore' });
    else {
      try { process.kill(-child.pid); }
      catch (error) { if (error.code !== 'ESRCH') console.error(`could not stop native inspection: ${error.message}`); }
    }
  };
  const timer = setTimeout(() => {
    console.error(`timed out after ${options.timeout}s; is prepare() waiting on something?`);
    stop();
    process.exit(1);
  }, options.timeout * 1000);
  child.on('error', (error) => fail(`could not start ${options.native ?? 'the Celesta runtime'}: ${error.message}`));
  const lines = readline.createInterface({ input: child.stdout })[Symbol.asyncIterator]();
  let ready;
  const next = async () => {
    const { value, done } = await lines.next();
    if (done && options.native && !ready) {
      throw new Error(
        `${options.native} exited before the inspection handshake (see the messages above). ` +
          'If it does not support --inspect, update Celesta, or check this entry with PNG frames / a contact sheet ' +
          '(Celesta-export --react <entry> --every 15 --contact-sheet out.png) instead',
      );
    }
    if (done) throw new Error('the Celesta runtime exited unexpectedly (see the messages above)');
    return JSON.parse(value);
  };
  const send = (message) => child.stdin.write(`${JSON.stringify(message)}\n`);

  // `prepare()` may open a side channel before the handshake, and an entry that
  // does is still loadable: answer what can be answered here and let it fall
  // back. Text shaping is not one of those things — it lives in the renderer,
  // which Node-only inspection has no access to — so measurements are refused
  // and the entry's own offline fallback path takes over. --native answers
  // both in the exporter.
  const receive = async () => {
    for (;;) {
      const message = await next();
      removeDecodedAudio();
      if (message.probeMedia) {
        send(probeMedia(message.probeMedia.path));
        continue;
      }
      if (message.decodeAudio) {
        send(decodeAudio(message.decodeAudio.path));
        continue;
      }
      if (message.measureText) {
        send({ error: TEXT_UNSUPPORTED });
        continue;
      }
      if (message.error?.includes(TEXT_UNSUPPORTED)) message.status = 'unsupported';
      return message;
    }
  };
  const finish = async (code) => {
    clearTimeout(timer);
    child.stdin.end();
    // The exporter drops its bridge, ending Node, once stdin closes.
    if (options.native) await exited;
    else child.kill();
    removeDecodedAudio();
    process.exit(code);
  };

  let problems = 0;
  let unsupported = 0;
  for (;;) {
    const message = await receive();
    if (message.error) {
      if (options.json) console.log(JSON.stringify(message, null, 2));
      else console.error(`${message.status === 'unsupported' ? 'UNSUPPORTED inspection' : 'ERROR loading entry'}: ${message.error}`);
      await finish(message.status === 'unsupported' ? 2 : 1);
    }
    if (message.invalidProperties) {
      console.error('ERROR invalid project properties:');
      for (const { key, source, message: text } of message.invalidProperties) {
        console.error(`  - ${source}: ${key ? `${key}: ` : ''}${text}`);
      }
      await finish(1);
    }
    ready = message;
    break;
  }

  const { config } = ready;
  if (options.loop) await finish(await checkLoop(config, send, receive, options, ready));
  const fps = config.frameRate.numerator / config.frameRate.denominator;
  const total = config.durationInFrames;
  let frames;
  if (options.every) {
    frames = [];
    for (let frame = 0; frame < total; frame += options.every) frames.push(frame);
    if (frames.at(-1) !== total - 1) frames.push(total - 1);
  } else {
    frames = options.frames ?? [0, Math.floor(total / 2), total - 1];
  }
  frames = [...new Set(frames.map((frame) => (frame < 0 ? total + frame : frame)))].filter((frame) => frame >= 0 && frame < total);

  const warnings = [];
  if (config.width % 2 || config.height % 2) warnings.push(`width/height ${config.width}×${config.height} must be even for video export`);

  if (options.json) {
    const results = [];
    for (const frame of frames) {
      send({ time: { value: frame * config.frameRate.denominator, timescale: config.frameRate.numerator } });
      const response = await receive();
      results.push({ frame, ...response });
      if (response.fatal) break;
    }
    console.log(JSON.stringify({ ready, frames: results }, null, 2));
    problems += results.filter((result) => result.error && result.status !== 'unsupported').length;
    unsupported += results.filter((result) => result.status === 'unsupported').length;
  } else {
    console.log(`compositions: ${(ready.compositions ?? []).map((item) => item.id).join(', ')} (selected: ${ready.compositionId ?? 'default'})`);
    console.log(`composition: ${config.width}×${config.height} @ ${fmt(fps)} fps, ${total} frames (${fmt(total / fps)} s)`);
    const schemas = Object.keys(ready.componentSchemas ?? {});
    if (schemas.length) console.log(`registered components with schemas: ${schemas.join(', ')}`);
    if (ready.propertySchema) console.log(`project properties: ${Object.keys(ready.propertySchema).join(', ')}`);

    const missing = new Set();
    const checkFile = (src) => {
      if (!src || /^https?:\/\//i.test(src)) return '';
      const resolved = path.isAbsolute(src) ? src : path.resolve(entryDir, src);
      if (existsSync(resolved)) return '';
      missing.add(src);
      return '  ← MISSING FILE';
    };

    for (const frame of frames) {
      send({ time: { value: frame * config.frameRate.denominator, timescale: config.frameRate.numerator } });
      const response = await receive();
      console.log(`\n── frame ${frame} (${fmt(frame / fps)} s) ──`);
      if (response.error) {
        if (response.status === 'unsupported') {
          unsupported += 1;
          console.log(`UNSUPPORTED inspection: ${response.error}`);
        } else {
          problems += 1;
          console.log(`ERROR: ${response.error}`);
        }
        if (response.fatal) break;
        continue;
      }
      const { scene, audio } = response;
      for (const font of scene.fonts ?? []) {
        console.log(`font ${font.id}: ${font.location.path ?? font.location.url}${checkFile(font.location.path)}`);
      }
      if (scene.layers.length === 0) console.log('(no layers: the frame is black)');
      printLayers(scene.layers, 0, checkFile, warnings, scene.shaders ?? []);
      for (const clip of audio) {
        console.log(
          `audio ${clip.src} from ${fmt(clip.start)} s for ${fmt(clip.duration)} s` +
            `${clip.sourceStart ? `, starting ${fmt(clip.sourceStart)} s into the file` : ''}` +
            `${typeof clip.volume === 'number' && clip.volume !== 1 ? `, volume ${fmt(clip.volume)}` : ''}` +
            `${typeof clip.volume === 'object' ? ', volume keyframes' : ''}` +
            `${clip.fadeIn ? `, fading in over ${fmt(clip.fadeIn)} s` : ''}` +
            `${clip.fadeOut ? `, fading out over ${fmt(clip.fadeOut)} s` : ''}` +
            `${clip.loop ? ', looping' : ''}${clip.muted ? ', muted' : ''}${checkFile(clip.src)}`,
        );
      }
    }
    problems += missing.size;
    console.log('');
    for (const warning of new Set(warnings)) console.log(`warning: ${warning}`);
    for (const src of missing) console.log(`missing: ${src} (relative paths resolve from ${entryDir})`);
    if (problems) console.log(`${problems} problem(s) found`);
    else if (unsupported) console.log(`${unsupported} frame(s) measure text and could not be inspected; use --native <Celesta-export> or a PNG export`);
    else console.log('OK: entry loads and every inspected frame evaluates');
  }

  await finish(problems ? 1 : unsupported ? 2 : 0);
}

// ------------------------------------------------------------ loop check

/** At most this many differences are printed per compared pair. */
const LOOP_DIFFERENCE_LIMIT = 20;

/**
 * Compares frame N (one past the last, evaluated as if the video went on)
 * with frame 0, and N + 1 with 1, so both the picture and its motion carry
 * across the seam. Returns the exit code: 1 when anything differs or fails,
 * 2 when only text measurement stopped it.
 */
async function checkLoop(config, send, receive, options, ready) {
  const fps = config.frameRate.numerator / config.frameRate.denominator;
  const total = config.durationInFrames;
  const timeOf = (frame) => ({ value: frame * config.frameRate.denominator, timescale: config.frameRate.numerator });
  const pairs = [];
  let problems = 0;
  let unsupported = 0;
  for (const start of total > 1 ? [0, 1] : [0]) {
    send({ time: timeOf(start) });
    const first = await receive();
    send({ time: timeOf(total + start), openEnd: true });
    const past = await receive();
    if (!first.error && !past.error && past.openEnd !== true) {
      fail(
        options.native
          ? `${options.native} cannot evaluate frames past the end. Update Celesta, or pass a newer build to --native.`
          : 'This Celesta runtime cannot evaluate frames past the end. Update Celesta, or pass --runtime <cli.js> from a newer build.',
      );
    }
    const pair = { frame: total + start, against: start };
    if (first.error || past.error) {
      const failed = first.error ? first : past;
      pair.error = failed.error;
      if (failed.status === 'unsupported') {
        pair.status = 'unsupported';
        unsupported += 1;
      } else {
        problems += 1;
      }
    } else {
      // The clock is the one thing that must differ.
      const { time: _firstTime, ...firstScene } = first.scene;
      const { time: _pastTime, ...pastScene } = past.scene;
      pair.differences = [];
      diffScenes(pastScene, firstScene, '', pair.differences);
      problems += pair.differences.length;
    }
    pairs.push(pair);
  }

  const code = problems ? 1 : unsupported ? 2 : 0;
  if (options.json) {
    console.log(JSON.stringify({ ready, loop: pairs }, null, 2));
    return code;
  }
  console.log(`composition: ${config.width}×${config.height} @ ${fmt(fps)} fps, ${total} frames (${fmt(total / fps)} s)`);
  console.log(`loop check: frames ${total} and up are evaluated as if the video went on, then compared with the start\n`);
  for (const pair of pairs) {
    const label = `frame ${pair.frame} vs frame ${pair.against}`;
    if (pair.error) {
      console.log(`${label}: ${pair.status === 'unsupported' ? 'UNSUPPORTED inspection' : 'ERROR'}: ${pair.error}`);
    } else if (pair.differences.length === 0) {
      console.log(`${label}: same Scene`);
    } else {
      console.log(`${label}: ${pair.differences.length} difference(s)`);
      for (const { path: where, end, start } of pair.differences.slice(0, LOOP_DIFFERENCE_LIMIT)) {
        console.log(`  ${where || '(scene)'}: ${formatValue(end)} at frame ${pair.frame}, ${formatValue(start)} at frame ${pair.against}`);
      }
      if (pair.differences.length > LOOP_DIFFERENCE_LIMIT) {
        console.log(`  … and ${pair.differences.length - LOOP_DIFFERENCE_LIMIT} more (--json lists them all)`);
      }
    }
  }
  console.log('');
  if (pairs.some((pair) => pair.error && pair.status !== 'unsupported')) console.log(`${problems} problem(s) found`);
  else if (problems) console.log('The loop has a seam: the frames above should match. Sound is not compared; see loops.md.');
  else if (unsupported) console.log('These frames measure text, so the loop could not be checked; compare the seam in a contact sheet instead (loops.md)');
  else console.log('OK: the last frame runs into the first (picture only; sound is not compared)');
  return code;
}

const HEX_COLOR = /^#(?:[0-9a-f]{6}|[0-9a-f]{8})$/i;

/**
 * Collects where two Scene values differ. Numbers may differ by rounding
 * (one part in a million, at least 1e-6) and hex colors by one step per
 * channel, as periodic math and color interpolation round a little
 * differently a whole period later. Layer ids are not compared: they name
 * the place in the tree (the second of two <Series> scenes, say) and do not
 * change the picture. Lists of different lengths are reported once, as the
 * layer types on each side.
 */
function diffScenes(end, start, where, out) {
  if (typeof end === 'number' && typeof start === 'number') {
    if (Math.abs(end - start) > 1e-6 * Math.max(1, Math.abs(end), Math.abs(start))) out.push({ path: where, end, start });
    return;
  }
  if (typeof end === 'string' && typeof start === 'string' && HEX_COLOR.test(end) && HEX_COLOR.test(start) && end.length === start.length) {
    for (let i = 1; i < end.length; i += 2) {
      if (Math.abs(parseInt(end.slice(i, i + 2), 16) - parseInt(start.slice(i, i + 2), 16)) > 1) {
        out.push({ path: where, end, start });
        return;
      }
    }
    return;
  }
  if (Array.isArray(end) && Array.isArray(start)) {
    if (end.length !== start.length) {
      out.push({ path: where, end: describeList(end), start: describeList(start) });
      return;
    }
    for (let i = 0; i < end.length; i += 1) {
      const id = typeof end[i]?.id === 'string' ? `(${end[i].id})` : '';
      diffScenes(end[i], start[i], `${where}[${i}]${id}`, out);
    }
    return;
  }
  if (end && start && typeof end === 'object' && typeof start === 'object' && !Array.isArray(end) && !Array.isArray(start)) {
    for (const key of new Set([...Object.keys(end), ...Object.keys(start)])) {
      if (key === 'id') continue;
      diffScenes(end[key], start[key], where ? `${where}.${key}` : key, out);
    }
    return;
  }
  if (end !== start) out.push({ path: where, end, start });
}

function describeList(list) {
  if (list.length === 0) return 'no layers';
  if (!list.every((item) => typeof item?.content?.type === 'string')) return `${list.length} items`;
  const types = list.slice(0, 6).map((item) => item.content.type);
  return `${list.length} layers: ${types.join(', ')}${list.length > types.length ? ', …' : ''}`;
}

function formatValue(value) {
  if (value === undefined) return '(absent)';
  if (typeof value === 'number') return Number.isInteger(value) ? String(value) : String(Number(value.toFixed(6)));
  const text = JSON.stringify(value);
  return text.length > 60 ? `${text.slice(0, 57)}…` : text;
}

function fmt(value) {
  return Number.isInteger(value) ? String(value) : value.toFixed(3).replace(/0+$/, '').replace(/\.$/, '');
}

function describeTransform(layer, sources) {
  const { position, scale, rotation, anchor } = layer.transform;
  const parts = [`at (${fmt(position.x)}, ${fmt(position.y)})`];
  if (anchor.x !== 0 || anchor.y !== 0) parts.push(`anchor (${fmt(anchor.x)}, ${fmt(anchor.y)})`);
  // Compact JSON omits the origin at (0, 0).
  const origin = layer.transform.origin ?? { x: 0, y: 0 };
  if (origin.x !== 0 || origin.y !== 0) parts.push(`origin (${fmt(origin.x)}, ${fmt(origin.y)})`);
  if (scale.x !== 1 || scale.y !== 1) parts.push(scale.x === scale.y ? `scale ${fmt(scale.x)}` : `scale (${fmt(scale.x)}, ${fmt(scale.y)})`);
  if (rotation) parts.push(`rotate ${fmt(rotation)}°`);
  // Compact JSON omits these at their defaults.
  const skew = layer.transform.skew ?? { x: 0, y: 0 };
  if (skew.x !== 0 || skew.y !== 0) parts.push(`skew (${fmt(skew.x)}°, ${fmt(skew.y)}°)`);
  const { rotationX = 0, rotationY = 0, perspective, backfaceVisibility } = layer.transform;
  if (rotationX) parts.push(`rotate X ${fmt(rotationX)}°`);
  if (rotationY) parts.push(`rotate Y ${fmt(rotationY)}°`);
  if (Number.isFinite(perspective) && perspective > 0) parts.push(`perspective ${fmt(perspective)}`);
  if (backfaceVisibility === 'hidden') parts.push('backface hidden');
  if (layer.opacity !== 1) parts.push(`opacity ${fmt(layer.opacity)}`);
  if (layer.blendMode && layer.blendMode !== 'normal') parts.push(`blend ${layer.blendMode}`);
  const effects = layer.effects ?? {};
  if (effects.blur) parts.push(`blur ${fmt(effects.blur)}`);
  if (effects.shadow) parts.push(`shadow ${effects.shadow.color}/${fmt(effects.shadow.blur)} (${fmt(effects.shadow.offsetX)}, ${fmt(effects.shadow.offsetY)})`);
  if (effects.glow) parts.push(`glow ${effects.glow.color}/${fmt(effects.glow.blur)}`);
  // Older runtimes send `shaders`, or a single `shader`, entries without a `type`.
  const filters = effects.filters ?? effects.shaders ?? (effects.shader ? [effects.shader] : []);
  if (filters.length > 0) parts.push(`filters ${filters.map((filter) => describeFilter(filter, sources)).join(' → ')}`);
  return parts.join(' ');
}

/** A custom shader as `describeShader` gives it, or a color filter as `brightness(1.2)`. */
function describeFilter(filter, sources) {
  switch (filter.type ?? 'shader') {
    case 'shader': return describeShader(filter, sources);
    case 'hue': return `hue(${fmt(filter.degrees)}°)`;
    case 'tint': return `tint(${filter.color}, ${fmt(filter.amount)})`;
    case 'vignette': return `vignette(${fmt(filter.amount)}, radius ${fmt(filter.radius)}, softness ${fmt(filter.softness)})`;
    case 'pixelate': return `pixelate(${fmt(filter.size)} px)`;
    case 'halftone': return `halftone(${fmt(filter.size)} px, ${fmt(filter.angle)}°)`;
    case 'chromaticAberration': return `chromaticAberration(${fmt(filter.offset)} px, ${fmt(filter.angle)}°)`;
    case 'wave':
      return `wave(${fmt(filter.amplitude)} px, wavelength ${fmt(filter.wavelength)} px, phase ${fmt(filter.phase)}°, ${fmt(filter.angle)}°)`;
    case 'noise': return `noise(${fmt(filter.amount)}, seed ${fmt(filter.seed)}${filter.monochrome === false ? ', color' : ''})`;
    case 'scanlines':
      return `scanlines(${fmt(filter.amount)}, spacing ${fmt(filter.spacing)} px, thickness ${fmt(filter.thickness)}, offset ${fmt(filter.offset)} px)`;
    case 'zoomBlur': return `zoomBlur(${fmt(filter.amount)}, centre (${fmt(filter.centerX)}, ${fmt(filter.centerY)}))`;
    case 'chromaKey':
      return `chromaKey(${filter.color}, tolerance ${fmt(filter.tolerance)}, softness ${fmt(filter.softness)}, spill ${fmt(filter.spill)})`;
    default: return `${filter.type}(${fmt(filter.amount)})`;
  }
}

/** `name(param=value, …) pad N`, naming the parameters the scene declares. */
function describeShader(shader, sources) {
  const source = sources.find((candidate) => candidate.id === shader.id);
  const values = shader.params ?? [];
  const components = { f32: 1, vec2: 2, vec3: 3, vec4: 4 };
  let at = 0;
  const params = source
    ? (source.params ?? []).map((param) => {
      const value = values.slice(at, (at += components[param.type] ?? 1)).map(fmt);
      return `${param.name}=${value.length === 1 ? value[0] : `(${value.join(', ')})`}`;
    })
    : values.map(fmt);
  return `${source?.name ?? shader.id}(${params.join(', ')})${shader.padding ? ` pad ${fmt(shader.padding)}` : ''}`;
}

function describePaint(paint) {
  if (!paint) return '';
  if (paint.type === 'solid') return paint.color;
  const stops = (paint.stops ?? []).map((stop) => stop.color).join('→');
  return `${paint.type}(${stops})`;
}

function printLayers(layers, depth, checkFile, warnings, sources) {
  const indent = '  '.repeat(depth);
  for (const layer of layers) {
    const content = layer.content;
    const where = describeTransform(layer, sources);
    switch (content.type) {
      case 'text': {
        const style = content.style ?? {};
        const family = Array.isArray(style.fontFamily) ? style.fontFamily.join(', ') : style.fontFamily;
        const font = [family ?? 'default font', style.fontSize && `${style.fontSize}px`, style.fontWeight].filter(Boolean).join(' ');
        const color = style.fill ? ` ${describePaint(style.fill)}` : '';
        const extras = [
          content.maxWidth && `maxWidth ${fmt(content.maxWidth)}`,
          style.lineBreak === 'phrase' && 'lineBreak phrase',
          content.baselineAnchor && 'baseline anchor',
        ].filter(Boolean);
        console.log(`${indent}text ${JSON.stringify(content.text)} ${where} [${font}${color}${extras.map((extra) => `, ${extra}`).join('')}]`);
        break;
      }
      case 'rect':
        console.log(`${indent}rect ${fmt(content.width)}×${fmt(content.height)} ${where}${content.fill ? ` fill ${describePaint(content.fill)}` : ''}${content.stroke ? ` stroke ${describePaint(content.stroke.paint)}/${fmt(content.stroke.width)}` : ''}${content.cornerRadius ? ` radius ${fmt(content.cornerRadius)}` : ''}`);
        break;
      case 'path':
        console.log(`${indent}path ${content.commands.length} commands ${where}${content.fill ? ` fill ${describePaint(content.fill)}` : ''}${content.stroke ? ` stroke ${describePaint(content.stroke.paint)}/${fmt(content.stroke.width)}` : ''}`);
        break;
      case 'image':
      case 'video':
      case 'psd': {
        const src = content.asset.location.path ?? content.asset.location.url;
        let extra = '';
        if ((content.type === 'image' || content.type === 'video') && (content.width || content.height)) {
          extra = ` size ${content.width ? fmt(content.width) : 'auto'}×${content.height ? fmt(content.height) : 'auto'}${content.fit ? ` ${content.fit}` : ''}`;
        }
        if (content.type === 'video') extra += ` (source ${fmt(content.timing.sourceTimeSeconds)} s${content.timing.loop ? ', looping' : ''})`;
        if (content.type === 'psd') {
          extra = ` (${content.visibleLayers?.length ?? 0} visible layers${content.enabledLayers?.length ? `, shown ${content.enabledLayers.join(', ')}` : ''})`;
          if (!content.visibleLayers?.length) warnings.push(`PSD ${src} has no \`layers\`; it renders with its saved visibility`);
        }
        console.log(`${indent}${content.type} ${src} ${where}${extra}${checkFile(content.asset.location.path)}`);
        break;
      }
      case 'group': {
        const clip = content.clip;
        const clipText = clip ? ` clip ${fmt(clip.width)}×${fmt(clip.height)} at (${fmt(clip.x)}, ${fmt(clip.y)})${clip.cornerRadius ? ` radius ${fmt(clip.cornerRadius)}` : ''}` : '';
        console.log(`${indent}group ${where}${clipText}`);
        printLayers(content.layers, depth + 1, checkFile, warnings, sources);
        break;
      }
      case 'missingComponent':
        warnings.push(`component "${content.component}" is not registered; export will fail`);
        console.log(`${indent}UNREGISTERED COMPONENT ${content.component} ${where}`);
        break;
      default:
        console.log(`${indent}${content.type} ${where}`);
    }
  }
}

// ------------------------------------------------------------ PSD layers

function listPsdLayers(options) {
  const file = path.resolve(options.psd);
  if (!existsSync(file)) fail(`PSD not found: ${file}`);
  const { cli } = findRuntime(options, [path.dirname(file), process.cwd()]);
  const { readPsd } = createRequire(cli)('ag-psd');
  const psd = readPsd(readFileSync(file), { skipLayerImageData: true, skipCompositeImageData: true, skipThumbnail: true });
  console.log(`${path.basename(file)}: ${psd.width}×${psd.height}`);
  console.log('Paths for portrait `layers` and `lipSync` (folders end with /, [hidden] = saved hidden):');
  const walk = (children, prefix) => {
    for (const layer of children ?? []) {
      const layerPath = prefix ? `${prefix}/${layer.name}` : layer.name;
      console.log(`${layerPath}${layer.children ? '/' : ''}${layer.hidden ? '  [hidden]' : ''}`);
      if (layer.children) walk(layer.children, layerPath);
    }
  };
  walk(psd.children, '');
}

const options = parseArgs(process.argv.slice(2));
if (options.psd) listPsdLayers(options);
else inspectEntry(options).catch((error) => fail(`ERROR: ${error.message}`));
