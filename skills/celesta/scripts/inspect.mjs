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
// (measureText(), useTextMetrics(), <TextBox>, @celesta-plugin/code, …) and media
// probing, still without rendering pixels. --native <Celesta-export> picks the
// exporter; --no-native evaluates in Node only, which cannot shape text.
// Math.random(), Date.now(), performance.now() and new Date() called while
// rendering are reported as a `nondeterministic_render` warning (the runtime
// reseeds and freezes them per frame); --json lists it in `warnings`.
// The inspected frames' layers are also checked as every export checks them:
// scenes (chapters) under a second, a frame 0 of only solid rectangles, and
// text too close in color to the solid rectangles behind it, printed as
// `warning: <code>: …` lines (with --json, `quality`). They are warnings and
// do not change the exit code; Celesta-export --check-quality measures the
// rendered pixels (flashing, contrast, short shots).
// Exit code 1 means the entry failed to load, a frame failed, or a
// referenced local file is missing. Exit code 2 means only that text had to
// be measured, which needs the exporter or a PNG export.

import { spawn, spawnSync } from 'node:child_process';
import { existsSync, mkdtempSync, readdirSync, readFileSync, rmSync, statSync, writeFileSync } from 'node:fs';
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
  '@celesta-plugin/code, and subtitle.render metrics fail here; inspect with --native <Celesta-export>, ' +
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
      for (const api of message.nondeterministic ?? []) nondeterministic.add(api);
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
    // A large --json report to a pipe is still being written: exiting now
    // would cut it off.
    await new Promise((resolve) => process.stdout.write('', resolve));
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
  const chapters = config.chapters ?? [];
  const rough = chapters.filter((chapter) => chapter.status === 'rough');
  const range = (chapter) => `[${chapter.from}, ${chapter.from + chapter.durationInFrames})`;
  // Worded as celesta-export's rough_scenes warning words them.
  const place = (chapter) => `${chapter.name} ${range(chapter)}`;
  // The id too when the label differs: review comments are anchored to it.
  const label = (chapter) => (chapter.name && chapter.name !== chapter.id ? `${chapter.name} (${chapter.id})` : chapter.id);
  const span = (chapter) => `${label(chapter)} ${range(chapter)}${chapter.status === 'rough' ? ' rough' : ''}`;
  if (rough.length) {
    const lead = rough.length === 1
      ? '1 scene is still rough and draws a placeholder card'
      : `${rough.length} scenes are still rough and draw placeholder cards`;
    warnings.push(`${lead}: ${rough.map(place).join(', ')}; export warns about them (rough_scenes) and --final refuses them`);
  }
  const lint = sceneLint(config.frameRate);
  const qualityIssues = () => sortIssues([...shortChapters(config.chapters, config.frameRate), ...lint.finish()]);

  if (options.json) {
    const results = [];
    for (const frame of frames) {
      send({ time: { value: frame * config.frameRate.denominator, timescale: config.frameRate.numerator } });
      const response = await receive();
      results.push({ frame, ...response });
      if (response.scene) lint.observe(frame, response.scene);
      if (response.fatal) break;
    }
    const comments = reviewComments(entry, ready, config);
    console.log(JSON.stringify({ ready, frames: results, warnings: [...new Set([...warnings, ...nondeterminismWarnings()])], quality: qualityIssues(), ...(comments ? { comments: comments.counts } : {}) }, null, 2));
    problems += results.filter((result) => result.error && result.status !== 'unsupported').length;
    unsupported += results.filter((result) => result.status === 'unsupported').length;
  } else {
    console.log(`compositions: ${(ready.compositions ?? []).map((item) => item.id).join(', ')} (selected: ${ready.compositionId ?? 'default'})`);
    console.log(`composition: ${config.width}×${config.height} @ ${fmt(fps)} fps, ${total} frames (${fmt(total / fps)} s)`);
    const schemas = Object.keys(ready.componentSchemas ?? {});
    if (schemas.length) console.log(`registered components with schemas: ${schemas.join(', ')}`);
    if (ready.propertySchema) console.log(`project properties: ${Object.keys(ready.propertySchema).join(', ')}`);
    if (chapters.some((chapter) => chapter.status === 'rough' || chapter.desc)) {
      console.log(`chapters (${rough.length} of ${chapters.length} rough):`);
      for (const chapter of chapters) console.log(`  ${span(chapter)}${chapter.desc ? `: ${chapter.desc}` : ''}`);
    } else if (chapters.length) {
      console.log(`chapters: ${chapters.map(span).join(', ')}`);
    }
    const comments = reviewComments(entry, ready, config);
    if (comments) console.log(comments.line);

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
      lint.observe(frame, scene);
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
    for (const issue of qualityIssues()) warnings.push(`${issue.code}: ${issue.message}`);
    for (const warning of new Set([...warnings, ...nondeterminismWarnings()])) console.log(`warning: ${warning}`);
    for (const src of missing) console.log(`missing: ${src} (relative paths resolve from ${entryDir})`);
    if (problems) console.log(`${problems} problem(s) found`);
    else if (unsupported) console.log(`${unsupported} frame(s) measure text and could not be inspected; use --native <Celesta-export> or a PNG export`);
    else console.log('OK: entry loads and every inspected frame evaluates');
  }

  await finish(problems ? 1 : unsupported ? 2 : 0);
}

// ------------------------------------------------------------ determinism

/** The clock and random APIs the entry called while rendering, as the runtime reported them. */
const nondeterministic = new Set();
const NONDETERMINISTIC_ORDER = ['Math.random()', 'Date.now()', 'performance.now()', 'new Date()'];

/**
 * The `nondeterministic_render` warning, worded as Celesta-export words it;
 * empty when the entry called none of them.
 */
function nondeterminismWarnings() {
  if (nondeterministic.size === 0) return [];
  const rank = (api) => (NONDETERMINISTIC_ORDER.includes(api) ? NONDETERMINISTIC_ORDER.indexOf(api) : NONDETERMINISTIC_ORDER.length);
  const apis = [...nondeterministic].sort((a, b) => rank(a) - rank(b) || (a < b ? -1 : a > b ? 1 : 0));
  const listed = apis.length === 1 ? apis[0]
    : apis.length === 2 ? `${apis[0]} and ${apis[1]}`
    : `${apis.slice(0, -1).join(', ')}, and ${apis.at(-1)}`;
  return [
    `nondeterministic_render: the entry calls ${listed} while rendering; Celesta reseeds Math.random() for every frame ` +
      "and freezes the clock at the frame's time, so these values follow the frame and not the run. " +
      'Use random(seed) from @celesta.studio/math, and useCurrentFrame() or useCurrentTime()',
  ];
}

// ------------------------------------------------------- review comments

/**
 * The review comments on the inspected composition, from the project
 * root's celesta-comments/ folder (the nearest folder with
 * celesta.config.json above the entry, else the entry's): a summary line
 * and the counts, or null without any. A comment is orphaned when its
 * chapter id is not declared any more. Without chapters in the config
 * (none declared, or an exporter older than the comments) orphans are not
 * counted (`orphaned: null`).
 */
function reviewComments(entry, ready, config) {
  const entryDir = path.dirname(path.resolve(entry));
  let root = entryDir;
  for (let dir = entryDir; ; dir = path.dirname(dir)) {
    if (existsSync(path.join(dir, 'celesta.config.json'))) {
      root = dir;
      break;
    }
    if (path.dirname(dir) === dir) break;
  }
  const folder = path.join(root, 'celesta-comments');
  let names;
  try {
    names = readdirSync(folder).filter((name) => name.endsWith('.json'));
  } catch {
    return null;
  }
  const key = path.relative(root, path.resolve(entry)).split(path.sep).join('/');
  const selected = ready.compositionId ?? 'default';
  const first = (ready.compositions ?? [])[0]?.id ?? selected;
  const chapters = Array.isArray(config.chapters) ? new Set(config.chapters.map((chapter) => chapter.id)) : null;
  const counts = { open: 0, resolved: 0, orphaned: 0 };
  for (const name of names) {
    let comment;
    try {
      comment = JSON.parse(readFileSync(path.join(folder, name), 'utf8'));
    } catch {
      continue;
    }
    if (comment?.entry !== key) continue;
    if (comment.composition ? comment.composition !== selected : selected !== first) continue;
    if (comment.resolved) {
      counts.resolved += 1;
      continue;
    }
    counts.open += 1;
    const chapter = comment.anchor?.chapter;
    if (chapters && chapter && !chapters.has(chapter)) counts.orphaned += 1;
  }
  if (counts.open + counts.resolved === 0) return null;
  if (!chapters) counts.orphaned = null;
  let line = `comments: ${counts.open} open${counts.orphaned ? ` (${counts.orphaned} orphaned)` : ''}, ${counts.resolved} resolved`;
  if (counts.open) line += `; read them with list_comments or in ${folder}`;
  return { line, counts };
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
    console.log(JSON.stringify({ ready, loop: pairs, warnings: nondeterminismWarnings() }, null, 2));
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
  for (const warning of nondeterminismWarnings()) console.log(`warning: ${warning}`);
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
      case 'plugin': {
        // Only the plugin's own program knows what it draws; show what it is given.
        const assets = Object.entries(content.assets ?? {}).map(
          ([name, asset]) => `${name} ${asset.location.path ?? asset.location.url}${checkFile(asset.location.path)}`,
        );
        const data = content.data === undefined ? '' : ` data ${JSON.stringify(content.data)}`;
        console.log(`${indent}plugin ${content.plugin} at ${fmt(content.time)} s ${where}${assets.length ? ` [${assets.join(', ')}]` : ''}${data}`);
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

// --------------------------------------------------------- quality checks
//
// The checks every Celesta export runs on the layers it evaluates
// (crates/exporter/src/quality/scene.rs), with the same codes, rules, and
// messages: scenes (outermost chapters) under a second, a frame 0 with
// nothing but solid rectangles, and text whose color is too close to the
// solid colors behind its anchor point (WCAG 1.4.3: 4.5:1, 3:1 for text at
// least 24 px, or 18.66 px bold, on a 1080-pixel short side). Flashing,
// rendered contrast, and short shots need pixels: Celesta-export
// --check-quality.

const TEXT_CONTRAST = 4.5;
const LARGE_TEXT_CONTRAST = 3;
const MIN_SCENE_SECONDS = 1;
// GpuRenderOptions::default().background: what shows where nothing is drawn.
const EXPORT_BACKGROUND = [20, 22, 28];
const BLANK_ADVICE = "it is the thumbnail and the video's first frame, so put the title or the subject on it from the start";

const linearChannel = (code) => {
  const c = code / 255;
  return c <= 0.04045 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4;
};
const luminance = ([r, g, b]) => 0.2126 * linearChannel(r) + 0.7152 * linearChannel(g) + 0.0722 * linearChannel(b);
const contrastRatio = (a, b) => (Math.max(a, b) + 0.05) / (Math.min(a, b) + 0.05);
const parseHex = (value) => {
  const match = /^#([0-9a-f]{6})([0-9a-f]{2})?$/i.exec(typeof value === 'string' ? value : '');
  if (!match) return null;
  const n = parseInt(match[1], 16);
  return [(n >> 16) & 255, (n >> 8) & 255, n & 255, match[2] ? parseInt(match[2], 16) : 255];
};
const hexOf = (color) => `#${color.map((c) => c.toString(16).padStart(2, '0').toUpperCase()).join('')}`;
const roundTo = (value, scale) => Math.round(value * scale) / scale;
// Rust's `{:?}` of a string, close enough for messages.
const quoted = (text) => JSON.stringify(text);

function frameSpan(from, to, frameRate) {
  const seconds = (frame) => roundTo((frame * frameRate.denominator) / frameRate.numerator, 1000);
  return { from, to, start: seconds(from), end: seconds(to) };
}

function describeSpans(spans) {
  const parts = spans.slice(0, 4).map((span) =>
    span.to === span.from + 1
      ? `frame ${span.from} (${span.start.toFixed(2)} s)`
      : `frames ${span.from}–${span.to - 1} (${span.start.toFixed(2)}–${span.end.toFixed(2)} s)`);
  if (spans.length > 4) parts.push(`and ${spans.length - 4} more`);
  return parts.join(', ');
}

/** Adds `frame` to `spans`, kept sorted and joined, whatever order frames come in. */
function pushFrame(spans, frame) {
  let index = spans.findIndex(([, to]) => to >= frame);
  if (index < 0) index = spans.length;
  const span = spans[index];
  if (span && span[0] <= frame) span[1] = Math.max(span[1], frame + 1);
  else if (span && span[0] === frame + 1) span[0] = frame;
  else spans.splice(index, 0, [frame, frame + 1]);
  const next = spans[index + 1];
  if (next && next[0] <= spans[index][1]) {
    spans[index][1] = Math.max(spans[index][1], next[1]);
    spans.splice(index + 1, 1);
  }
}

/** The outermost chapters (the scenes) shorter than a second. */
function shortChapters(chapters = [], frameRate) {
  const end = (chapter) => chapter.from + chapter.durationInFrames;
  const outermost = (index) => !chapters.some((other, otherIndex) =>
    otherIndex !== index && other.from <= chapters[index].from && end(chapters[index]) <= end(other) &&
    (otherIndex < index || other.from !== chapters[index].from || end(other) !== end(chapters[index])));
  return chapters.flatMap((chapter, index) => {
    if (!(chapter.durationInFrames > 0) || !outermost(index)) return [];
    const length = (chapter.durationInFrames * frameRate.denominator) / frameRate.numerator;
    if (length >= MIN_SCENE_SECONDS) return [];
    const name = chapter.name && chapter.name !== chapter.id ? `${quoted(chapter.name)} (${chapter.id})` : quoted(chapter.id);
    const span = frameSpan(chapter.from, end(chapter), frameRate);
    return [{
      code: 'short_scene', measured: 'layers',
      message: `chapter ${name} lasts ${length.toFixed(2)} s (${describeSpans([span])}), under ${MIN_SCENE_SECONDS} s; give each scene at least a second`,
      frames: [span], value: roundTo(length, 1000), limit: MIN_SCENE_SECONDS,
    }];
  });
}

/** A layer's map from its plane (relative to its anchor) to its parent's: [a, b, c, d, tx, ty]. */
function affineOf(transform = {}) {
  const position = transform.position ?? { x: 0, y: 0 };
  const scale = transform.scale ?? { x: 1, y: 1 };
  const origin = transform.origin ?? { x: 0, y: 0 };
  const radians = ((transform.rotation ?? 0) * Math.PI) / 180;
  const [s, c] = transform.rotation ? [Math.sin(radians), Math.cos(radians)] : [0, 1];
  const a = c * scale.x, b = s * scale.x, cc = -s * scale.y, d = c * scale.y;
  return [a, b, cc, d, position.x + origin.x - (a * origin.x + cc * origin.y), position.y + origin.y - (b * origin.x + d * origin.y)];
}
const multiply = ([a, b, c, d, tx, ty], [a2, b2, c2, d2, tx2, ty2]) =>
  [a * a2 + c * b2, b * a2 + d * b2, a * c2 + c * d2, b * c2 + d * d2, a * tx2 + c * ty2 + tx, b * tx2 + d * ty2 + ty];
const applyAffine = ([a, b, c, d, tx, ty], [x, y]) => [a * x + c * y + tx, b * x + d * y + ty];
const affineScale = ([a, b, c, d]) => Math.sqrt(Math.abs(a * d - b * c));
function invertAffine([a, b, c, d, tx, ty]) {
  const det = a * d - b * c;
  if (!Number.isFinite(det) || Math.abs(det) < 1e-12) return null;
  const [ia, ib, ic, id] = [d / det, -b / det, -c / det, a / det];
  return [ia, ib, ic, id, -(ia * tx + ic * ty), -(ib * tx + id * ty)];
}
const isFlat = (t = {}) => !t.skew?.x && !t.skew?.y && !t.rotationX && !t.rotationY && !(Number.isFinite(t.perspective) && t.perspective > 0);
const isNormal = (layer) => !layer.blendMode || layer.blendMode === 'normal';

/** The solid colors bordering a text's glyphs: its innermost stroke, its shadow, its glow. */
function haloColors(layer, style) {
  const colors = [];
  const strokes = (Array.isArray(style.stroke) ? style.stroke : style.stroke ? [style.stroke] : []).filter((stroke) => stroke.width > 0);
  const innermost = strokes.sort((a, b) => a.width - b.width)[0];
  for (const value of [innermost?.paint?.type === 'solid' ? innermost.paint.color : null, layer.effects?.shadow?.color, layer.effects?.glow?.color]) {
    const color = parseHex(value);
    if (color && color[3] >= 128) colors.push(color.slice(0, 3));
  }
  return colors;
}

const over = (behind, [r, g, b, alpha]) => behind.map((channel, i) => channel + ([r, g, b][i] - channel) * Math.min(Math.max(alpha, 0), 1));
const encode = (color) => color.map((channel) => Math.min(255, Math.max(0, Math.round(channel))));

function pathBounds(commands = []) {
  let bounds = null;
  const add = (x, y) => { bounds = bounds ? [Math.min(bounds[0], x), Math.min(bounds[1], y), Math.max(bounds[2], x), Math.max(bounds[3], y)] : [x, y, x, y]; };
  for (const command of commands) {
    if (command.type === 'close') continue;
    if (command.x1 !== undefined) add(command.x1, command.y1);
    if (command.x2 !== undefined) add(command.x2, command.y2);
    add(command.x, command.y);
  }
  return bounds;
}

/** One frame's walk through its layers in drawing order. */
function walkScene(scene) {
  // `clips` are the clips of the groups the walk is in; each backdrop keeps
  // those it was drawn through.
  const walk = { backdrops: [], clips: [], texts: [], content: false, flatRects: false, shortSide: Math.max(1, Math.min(scene.width, scene.height)) };
  const inBox = ({ inverse, bounds: [x0, y0, x1, y1] }, point) => {
    const [x, y] = applyAffine(inverse, point);
    return x0 <= x && x <= x1 && y0 <= y && y <= y1;
  };
  const covers = (backdrop, point) =>
    (backdrop.everywhere || inBox(backdrop, point)) && backdrop.clips.every((clip) => inBox(clip, point));
  const drawn = (backdrop) => walk.backdrops.push({ ...backdrop, clips: [...walk.clips] });
  const measureText = (layer, shown, style, world, opacity) => {
    if (!isNormal(layer) || style.colorRuns?.length) return;
    let fill = [255, 255, 255, 255];
    if (style.fill) {
      if (style.fill.type !== 'solid') return;
      fill = parseHex(style.fill.color);
      if (!fill) return;
    }
    const point = applyAffine(world, [0, 0]);
    // Clipped away there, the text shows nothing to measure.
    if (!walk.clips.every((clip) => inBox(clip, point))) return;
    let behind = [...EXPORT_BACKGROUND];
    for (const backdrop of walk.backdrops) {
      if (!covers(backdrop, point)) continue;
      if (!backdrop.color) return;
      behind = over(behind, backdrop.color);
    }
    behind = encode(behind);
    const fg = encode(over(behind, [fill[0], fill[1], fill[2], (fill[3] / 255) * opacity]));
    let ratio = contrastRatio(luminance(fg), luminance(behind));
    for (const halo of haloColors(layer, style)) ratio = Math.max(ratio, contrastRatio(luminance(fg), luminance(halo)));
    const scale = affineScale(world);
    const size = (Math.abs(style.fontSize ?? 32) * scale * 1080) / walk.shortSide;
    const large = size >= 24 || ((style.fontWeight ?? 400) >= 700 && size >= 18.66);
    const chars = [...shown];
    walk.texts.push({
      id: layer.id, sample: chars.slice(0, 24).join('') + (chars.length > 24 ? '…' : ''),
      ratio, limit: large ? LARGE_TEXT_CONTRAST : TEXT_CONTRAST, fg, bg: behind, opacity, scale,
    });
  };
  const visit = (layers, parent, parentOpacity, geometry, exact) => {
    for (const layer of layers ?? []) {
      const opacity = parentOpacity * Math.min(Math.max(layer.opacity ?? 1, 0), 1);
      const transform = layer.transform ?? {};
      const world = multiply(parent, affineOf(transform));
      if (opacity < 0.001 || affineScale(world) < 1e-6) continue;
      const known = geometry && isFlat(transform);
      const certain = exact && isNormal(layer);
      const anchor = transform.anchor ?? { x: 0.5, y: 0.5 };
      const anchored = (width, height) => multiply(world, [1, 0, 0, 1, -anchor.x * width, -anchor.y * height]);
      const content = layer.content ?? {};
      switch (content.type) {
        case 'group': {
          // Under an unknown map, the children are unknown anyway.
          const clip = content.clip && known ? content.clip : null;
          const inverse = clip ? invertAffine(world) : null;
          if (clip && !inverse) break;
          if (clip) walk.clips.push({ inverse, bounds: [clip.x, clip.y, clip.x + clip.width, clip.y + clip.height] });
          visit(content.layers, world, opacity, known, certain && !content.mask);
          if (clip) walk.clips.pop();
          break;
        }
        case 'rect': {
          const solid = content.fill?.type === 'solid' ? parseHex(content.fill.color) : null;
          if ((content.fill && !solid) || content.stroke) walk.content = true;
          else if (solid && solid[3] > 0) walk.flatRects = true;
          if (!content.fill) break;
          if (!known) { drawn({ everywhere: true }); break; }
          const inverse = invertAffine(anchored(content.width, content.height));
          if (!inverse) break;
          const filtered = (layer.effects?.filters?.length ?? 0) > 0 || (layer.effects?.blur ?? 0) > 0;
          drawn({
            inverse, bounds: [0, 0, content.width, content.height],
            color: solid && certain && !filtered ? [solid[0], solid[1], solid[2], (solid[3] / 255) * opacity] : null,
          });
          break;
        }
        case 'image':
        case 'video': {
          walk.content = true;
          const inverse = known && content.width !== undefined && content.height !== undefined ? invertAffine(anchored(content.width, content.height)) : undefined;
          if (inverse === null) break;
          drawn(inverse ? { inverse, bounds: [0, 0, content.width, content.height] } : { everywhere: true });
          break;
        }
        case 'path': {
          walk.content = true;
          const bounds = pathBounds(content.commands);
          if (!bounds) break;
          if (!known) { drawn({ everywhere: true }); break; }
          const inverse = invertAffine(world);
          if (inverse) drawn({ inverse, bounds });
          break;
        }
        case 'instances':
          if (content.count > 0) { walk.content = true; drawn({ everywhere: true }); }
          break;
        case 'text': {
          const style = content.style ?? {};
          const text = String(content.text ?? '');
          const shown = style.visibleCharacters === undefined || style.visibleCharacters === null ? text : [...text].slice(0, style.visibleCharacters).join('');
          if (!shown.trim()) break;
          walk.content = true;
          if (known && certain) measureText(layer, shown, style, world, opacity);
          break;
        }
        default:
          walk.content = true;
          drawn({ everywhere: true });
      }
    }
  };
  visit(scene.layers, [1, 0, 0, 1, 0, 0], 1, true, true);
  return walk;
}

const differs = (a, b) => Math.abs(a.opacity - b.opacity) > 1e-3 || Math.abs(a.scale - b.scale) > 1e-3;
function settle(record, frame, finding, moving) {
  if (moving || finding.ratio >= finding.limit) return;
  pushFrame(record.failing, frame);
  if (finding.ratio < record.worst) Object.assign(record, { worst: finding.ratio, limit: finding.limit, colors: [finding.fg, finding.bg] });
}

/** The layer checks over each inspected frame, as the exporter's `SceneLint`. */
function sceneLint(frameRate) {
  let firstFrame = null;
  const texts = new Map();
  return {
    observe(frame, scene) {
      const walk = walkScene(scene);
      if (frame === 0 && !walk.content) {
        firstFrame = {
          code: 'blank_first_frame', measured: 'layers',
          message: walk.flatRects ? `frame 0 shows only solid rectangles: ${BLANK_ADVICE}` : `frame 0 draws nothing but the background: ${BLANK_ADVICE}`,
          frames: [frameSpan(0, 1, frameRate)],
        };
      }
      // A text fading or scaling in or out is measured once it holds
      // still: a frame counts once the next one shows it did not move.
      for (const text of walk.texts) {
        if (!texts.has(text.id)) texts.set(text.id, { worst: Infinity, limit: text.limit, colors: [text.fg, text.bg], sample: text.sample, failing: [], pending: null });
        const record = texts.get(text.id);
        let movedIn = false;
        if (record.pending) {
          const { frame: last, finding, moved } = record.pending;
          movedIn = last + 1 === frame && differs(finding, text);
          settle(record, last, finding, moved || movedIn);
        }
        record.pending = { frame, finding: text, moved: movedIn };
      }
    },
    finish() {
      const issues = firstFrame ? [firstFrame] : [];
      for (const [id, record] of [...texts].sort(([a], [b]) => (a < b ? -1 : a > b ? 1 : 0))) {
        if (record.pending) settle(record, record.pending.frame, record.pending.finding, record.pending.moved);
        record.pending = null;
        if (!record.failing.length) continue;
        const spans = record.failing.map(([from, to]) => frameSpan(from, to, frameRate));
        issues.push({
          code: 'low_contrast_text', measured: 'layers',
          message: `text layer ${quoted(id)} (${quoted(record.sample)}) is ${hexOf(record.colors[0])} on ${hexOf(record.colors[1])} behind it: ${record.worst.toFixed(2)}:1, below ${record.limit}:1, at ${describeSpans(spans)}`,
          frames: spans, layer: id, value: roundTo(record.worst, 100), limit: record.limit,
        });
      }
      return issues;
    },
  };
}

const QUALITY_ORDER = ['flashing', 'red_flashing', 'low_contrast_text', 'short_scene', 'short_shot', 'blank_first_frame'];
function sortIssues(issues) {
  const first = (issue) => issue.frames?.[0]?.from ?? 0;
  return issues.sort((a, b) =>
    QUALITY_ORDER.indexOf(a.code) - QUALITY_ORDER.indexOf(b.code) || first(a) - first(b) || String(a.layer ?? '').localeCompare(String(b.layer ?? '')));
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
