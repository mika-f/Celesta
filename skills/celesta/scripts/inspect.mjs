#!/usr/bin/env node
// Checks a Celesta React composition without the GUI: loads the entry with
// Celesta's own runtime (the same bundler, prepare() call, and renderer
// protocol the app and exporter use), evaluates a few frames, and prints
// every layer, audio clip, and missing media file. Also lists PSD layer
// paths for portrait setup.
//
//   node inspect.mjs <entry.tsx> [--frames 0,45,-1] [--every N] [--json]
//   node inspect.mjs --psd-layers <file.psd>
//
// Options: --runtime <cli.js> and --node <node> override runtime discovery
// (also CELESTA_REACT_CLI / CELESTA_NODE). --timeout <seconds> (default 180).
// Exit code 1 means the entry failed to load, a frame failed, or a
// referenced local file is missing.

import { spawn, spawnSync } from 'node:child_process';
import { existsSync, readFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import { homedir } from 'node:os';
import * as path from 'node:path';
import * as readline from 'node:readline';

const USAGE = `usage: node inspect.mjs <entry.tsx> [--frames 0,45,-1] [--every N] [--json]
       node inspect.mjs --psd-layers <file.psd>
options: --runtime <cli.js>  --node <node>  --timeout <seconds>`;

function parseArgs(argv) {
  const options = { frames: null, every: null, json: false, psd: null, runtime: null, node: null, timeout: 180, entry: null };
  for (let i = 0; i < argv.length; i += 1) {
    const arg = argv[i];
    const value = () => {
      if (i + 1 >= argv.length) fail(`${arg} needs a value\n${USAGE}`);
      return argv[++i];
    };
    if (arg === '--frames') options.frames = value().split(',').map((part) => parseInt(part.trim(), 10));
    else if (arg === '--every') options.every = parseInt(value(), 10);
    else if (arg === '--json') options.json = true;
    else if (arg === '--psd-layers') options.psd = value();
    else if (arg === '--runtime') options.runtime = value();
    else if (arg === '--node') options.node = value();
    else if (arg === '--timeout') options.timeout = Number(value());
    else if (arg === '-h' || arg === '--help') { console.log(USAGE); process.exit(0); }
    else if (arg.startsWith('--')) fail(`unknown option ${arg}\n${USAGE}`);
    else options.entry = arg;
  }
  if (options.frames?.some(Number.isNaN)) fail('--frames takes comma-separated integers (negative counts from the end)');
  if (options.every !== null && !(options.every > 0)) fail('--every takes a positive integer');
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
  // A source checkout: <repo>/packages/react/dist/cli.js above the entry or cwd.
  for (const start of startDirs) {
    for (let dir = path.resolve(start); ; dir = path.dirname(dir)) {
      candidates.push(path.join(dir, 'packages/react/dist/cli.js'));
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
        '  source:  <repo>/packages/react/dist/cli.js (run pnpm install, codegen, build first)',
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
  return {
    media: {
      durationSeconds: seconds(data.format?.duration),
      video: video && { codec: video.codec_name, width: video.width, height: video.height, frameRate, durationSeconds: seconds(video.duration) },
      audio: data.streams
        .filter((stream) => stream.codec_type === 'audio')
        .map((stream) => ({ codec: stream.codec_name, sampleRate: Number(stream.sample_rate), channels: stream.channels, durationSeconds: seconds(stream.duration) })),
    },
  };
}

// ------------------------------------------------------------ inspection

async function inspectEntry(options) {
  const entry = path.resolve(options.entry);
  if (!existsSync(entry)) fail(`entry not found: ${entry}`);
  const entryDir = path.dirname(entry);
  const { cli, node } = findRuntime(options, [entryDir, process.cwd()]);
  if (!options.json) console.log(`runtime: ${cli}\nnode:    ${node}\nentry:   ${entry}\n`);

  const child = spawn(node, [cli, entry], { stdio: ['pipe', 'pipe', 'inherit'] });
  const timer = setTimeout(() => {
    console.error(`timed out after ${options.timeout}s; is prepare() waiting on something?`);
    child.kill();
    process.exit(1);
  }, options.timeout * 1000);
  child.on('error', (error) => fail(`could not start ${node}: ${error.message}`));
  const lines = readline.createInterface({ input: child.stdout })[Symbol.asyncIterator]();
  const next = async () => {
    const { value, done } = await lines.next();
    if (done) throw new Error('the Celesta runtime exited unexpectedly (see the messages above)');
    return JSON.parse(value);
  };
  const send = (message) => child.stdin.write(`${JSON.stringify(message)}\n`);

  let problems = 0;
  let ready;
  // prepare() may ask for media probes before the handshake.
  for (;;) {
    const message = await next();
    if (message.probeMedia) {
      send(probeMedia(message.probeMedia.path));
      continue;
    }
    if (message.error) {
      console.error(`ERROR loading entry: ${message.error}`);
      child.kill();
      process.exit(1);
    }
    ready = message;
    break;
  }

  const { config } = ready;
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
  if (config.width % 2 || config.height % 2) warnings.push(`width/height ${config.width}×${config.height} must be even for MP4 export`);

  if (options.json) {
    const results = [];
    for (const frame of frames) {
      send({ time: { value: frame * config.frameRate.denominator, timescale: config.frameRate.numerator } });
      results.push({ frame, ...(await next()) });
    }
    console.log(JSON.stringify({ ready, frames: results }, null, 2));
    problems += results.filter((result) => result.error).length;
  } else {
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
      const response = await next();
      console.log(`\n── frame ${frame} (${fmt(frame / fps)} s) ──`);
      if (response.error) {
        problems += 1;
        console.log(`ERROR: ${response.error}`);
        continue;
      }
      const { scene, audio } = response;
      for (const font of scene.fonts ?? []) {
        console.log(`font ${font.id}: ${font.location.path ?? font.location.url}${checkFile(font.location.path)}`);
      }
      if (scene.layers.length === 0) console.log('(no layers: the frame is black)');
      printLayers(scene.layers, 0, checkFile, warnings);
      for (const clip of audio) {
        console.log(
          `audio ${clip.src} from ${fmt(clip.start)} s for ${fmt(clip.duration)} s` +
            `${clip.sourceStart ? `, starting ${fmt(clip.sourceStart)} s into the file` : ''}` +
            `${typeof clip.volume === 'number' && clip.volume !== 1 ? `, volume ${fmt(clip.volume)}` : ''}` +
            `${typeof clip.volume === 'object' ? ', volume keyframes' : ''}` +
            `${clip.muted ? ', muted' : ''}${checkFile(clip.src)}`,
        );
      }
    }
    problems += missing.size;
    console.log('');
    for (const warning of new Set(warnings)) console.log(`warning: ${warning}`);
    for (const src of missing) console.log(`missing: ${src} (relative paths resolve from ${entryDir})`);
    console.log(problems ? `${problems} problem(s) found` : 'OK: entry loads and every inspected frame evaluates');
  }

  clearTimeout(timer);
  child.stdin.end();
  child.kill();
  process.exit(problems ? 1 : 0);
}

function fmt(value) {
  return Number.isInteger(value) ? String(value) : value.toFixed(3).replace(/0+$/, '').replace(/\.$/, '');
}

function describeTransform(layer) {
  const { position, scale, rotation, anchor } = layer.transform;
  const parts = [`at (${fmt(position.x)}, ${fmt(position.y)})`];
  if (anchor.x !== 0 || anchor.y !== 0) parts.push(`anchor (${fmt(anchor.x)}, ${fmt(anchor.y)})`);
  if (scale.x !== 1 || scale.y !== 1) parts.push(scale.x === scale.y ? `scale ${fmt(scale.x)}` : `scale (${fmt(scale.x)}, ${fmt(scale.y)})`);
  if (rotation) parts.push(`rotate ${fmt(rotation)}°`);
  if (layer.opacity !== 1) parts.push(`opacity ${fmt(layer.opacity)}`);
  return parts.join(' ');
}

function printLayers(layers, depth, checkFile, warnings) {
  const indent = '  '.repeat(depth);
  for (const layer of layers) {
    const content = layer.content;
    const where = describeTransform(layer);
    switch (content.type) {
      case 'text': {
        const style = content.style ?? {};
        const font = [style.fontFamily ?? 'default font', style.fontSize && `${style.fontSize}px`, style.fontWeight].filter(Boolean).join(' ');
        const color = style.fill?.color ? ` ${style.fill.color}` : '';
        console.log(`${indent}text ${JSON.stringify(content.text)} ${where} [${font}${color}${content.maxWidth ? `, maxWidth ${content.maxWidth}` : ''}]`);
        break;
      }
      case 'rect':
        console.log(`${indent}rect ${fmt(content.width)}×${fmt(content.height)} ${where}${content.fill ? ` fill ${content.fill.color}` : ''}${content.stroke ? ` stroke ${content.stroke.paint.color}/${content.stroke.width}` : ''}${content.cornerRadius ? ` radius ${content.cornerRadius}` : ''}`);
        break;
      case 'image':
      case 'video':
      case 'psd': {
        const src = content.asset.location.path ?? content.asset.location.url;
        let extra = '';
        if (content.type === 'video') extra = ` (source ${fmt(content.timing.sourceTimeSeconds)} s)`;
        if (content.type === 'psd') {
          extra = ` (${content.visibleLayers?.length ?? 0} visible layers${content.enabledLayers?.length ? `, mouth ${content.enabledLayers.join(', ')}` : ''})`;
          if (!content.visibleLayers?.length) warnings.push(`PSD ${src} has no \`layers\`; it renders with its saved visibility`);
        }
        console.log(`${indent}${content.type} ${src} ${where}${extra}${checkFile(content.asset.location.path)}`);
        break;
      }
      case 'group':
        console.log(`${indent}group ${where}`);
        printLayers(content.layers, depth + 1, checkFile, warnings);
        break;
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
