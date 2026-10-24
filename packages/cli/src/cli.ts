import { Console } from 'node:console';
import * as fs from 'node:fs';
import { tmpdir } from 'node:os';
import * as path from 'node:path';
import { pathToFileURL } from 'node:url';

import { listProjectProperties } from '@celesta/react';
import type {
  ComponentPropertySchema,
  CompositionConfig,
  CompositionInfo,
  Layer,
  ProjectPropertyField,
  Scene,
  TextMetrics,
  Time,
} from '@celesta/react';
import {
  createResolver,
  holdProjectPropertyValues,
  listComponentSchemas,
  mount,
  prepareDependencies,
  setAudioDecoder,
  setMediaProbe,
  setProjectPropertyValues,
  setTextMeasurer,
} from '@celesta/react/internal';
import type {
  AudioClipDescriptor,
  ComponentResolutionRequest,
  ComponentResolutions,
  DecodedAudio,
  EntryComponent,
  LayerSourceReport,
  MeasureTextRequest,
  MountedComposition,
  OccurrenceIndex,
  PrepareDependency,
  ProbedMediaInfo,
  ProjectFrame,
  ResolutionRuntime,
  Resolver,
  SubtitleDescriptor,
} from '@celesta/react/internal';

import { resolvePropertyInputs } from './property-inputs';
import type { PropertyInputLayer, PropertyIssue } from './property-inputs';

// stdout is the JSON request/response channel the Rust bridge
// (crates/react-bridge) parses one message at a time; anything else written
// there corrupts the protocol and fails the export. Route every `console`
// method to stderr — which the bridge inherits straight to the user's
// terminal — so a `console.log` left in a composition is evaluated and
// printed to the console instead of breaking the run. A `Console` lacks the
// global console's inspector-only methods (`timeStamp`, `createTask`, ...),
// which write nothing to either stream; React's development build calls
// `console.timeStamp` on every render, so those are kept.
globalThis.console = Object.assign(
  Object.create(console) as Console,
  new Console({ stdout: process.stderr, stderr: process.stderr }),
);

// The protocol's messages are exported for the tests that drive the built CLI.

export interface FrameRequest {
  time: Time;
  project?: ProjectFrame;
  /**
   * Sent by bridges that fill omitted fields with their defaults: transform
   * fields, an opacity of 1, and a rect's corner radius of 0.
   */
  compactTransforms?: boolean;
  /**
   * Sent by bridges that keep `<Static>` subtrees: refer to them by key.
   * `decoded` is the `seq` of the last frame the bridge decoded.
   */
  statics?: { decoded: number };
  /**
   * Evaluate a frame past the end as if the video went on (inspect.mjs
   * --loop). The response echoes `openEnd: true`, so a caller can tell an
   * older runtime that ignored it.
   */
  openEnd?: boolean;
  /** Draw debug guides, for PNG exports with `--debug-guides`. */
  debugGuides?: boolean;
  /**
   * Sent only by the editor's preview: `useIsPreview()` is true. Absent from
   * exports, inspection and older bridges.
   */
  preview?: boolean;
}

export interface ResolveRequest {
  components: ComponentResolutionRequest[];
  /** Absent from older bridges; resolved components then see frame 0. */
  runtime?: ResolutionRuntime;
}

/** Asks which JSX produced a layer; see `MountedComposition.layerSource`. */
export interface LayerSourceRequest {
  layerSource: string;
  time: Time;
  preview?: boolean;
}

/** Asks for the subtitles of `frames` frames from `from`; see `MountedComposition.collectSubtitles`. */
export interface CollectSubtitlesRequest {
  collectSubtitles: { from: number; frames: number };
}

/** Asks where each element renders in `frames` frames from `from`; see `MountedComposition.collectOccurrences`. */
export interface CollectOccurrencesRequest {
  collectOccurrences: { from: number; frames: number; preview?: boolean };
}

export type Request =
  | FrameRequest
  | ResolveRequest
  | LayerSourceRequest
  | CollectSubtitlesRequest
  | CollectOccurrencesRequest
  | { collectAudio: true };

/** The first message: what the entry declares. */
export interface ReadyMessage {
  config: CompositionConfig;
  compositionId: string;
  compositions: readonly CompositionInfo[];
  componentSchemas: Record<string, ComponentPropertySchema>;
  propertySchema: Record<string, ProjectPropertyField> | null;
  dependencies: PrepareDependency[];
}

/** The answer to a `FrameRequest`. */
export interface FrameMessage {
  scene: Scene;
  audio: AudioClipDescriptor[];
  subtitles?: SubtitleDescriptor[];
  seq?: number;
  statics?: { key: number; layers: Layer[] }[];
  droppedStatics?: number[];
  /** Echoes `FrameRequest.openEnd`. */
  openEnd?: true;
}

/** Every message written to stdout. */
export type Message =
  | ReadyMessage
  | FrameMessage
  | { collectedAudio: AudioClipDescriptor[] }
  | { collectedSubtitles: SubtitleDescriptor[] }
  | { collectedOccurrences: OccurrenceIndex }
  | { layerSource: LayerSourceReport | { chain: null } }
  | ComponentResolutions
  | { probeMedia: { path: string } }
  | { decodeAudio: { path: string } }
  | { measureText: MeasureTextRequest }
  | { invalidProperties: PropertyIssue[] }
  | { error: string };

interface ProbeMediaResponse {
  media?: ProbedMediaInfo;
  error?: string;
}

interface DecodeAudioResponse {
  /** A temporary file of mono little-endian f32 samples, which Celesta removes. */
  audio?: { path: string; sampleRate: number; frames: number };
  error?: string;
}

function isLayerSourceRequest(request: Request): request is LayerSourceRequest {
  return typeof (request as LayerSourceRequest).layerSource === 'string';
}

function isResolveRequest(request: Request): request is ResolveRequest {
  return Array.isArray((request as ResolveRequest).components);
}

async function main(): Promise<void> {
  const [entry, ...args] = process.argv.slice(2);
  const options: string[] = [];
  let compositionId: string | undefined;
  for (let i = 0; i < args.length; i++) {
    if (args[i] === '--composition' && args[i + 1] !== undefined) {
      compositionId = args[++i];
    } else options.push(args[i]);
  }
  if (!entry || options.some((option) => ![PROPERTIES_STDIN, LENGTH_PREFIXED, SOURCE_LOCATIONS].includes(option))) {
    process.stderr.write(`usage: celesta-react-render <entry-file> [--composition ID] [${PROPERTIES_STDIN}] [${LENGTH_PREFIXED}] [${SOURCE_LOCATIONS}]\n`);
    process.exitCode = 1;
    return;
  }

  lengthPrefixed = options.includes(LENGTH_PREFIXED);

  const entryPath = path.resolve(entry);
  // Local-file helpers used during `prepare()` (loadLipSync, loadPsdPreset)
  // resolve relative paths against this, matching how the Rust renderer
  // resolves a relative `<Audio>`/`<Image>` src against the entry directory.
  process.env.CELESTA_REACT_ENTRY_DIR = path.dirname(entryPath);

  const lines = new ProtocolLines();
  // Prepare-time probes and measurements share one request/response channel.
  let requestQueue = Promise.resolve();
  setMediaProbe((mediaPath) => {
    const result = requestQueue.then(() => requestMediaProbe(lines, mediaPath));
    requestQueue = result.then(
      () => undefined,
      () => undefined,
    );
    return result;
  });

  setAudioDecoder((mediaPath) => {
    const result = requestQueue.then(() => requestDecodedAudio(lines, mediaPath));
    requestQueue = result.then(
      () => undefined,
      () => undefined,
    );
    return result;
  });

  setTextMeasurer((request) => {
    const result = requestQueue.then(() => requestTextMeasure(lines, request));
    requestQueue = result.then(
      () => undefined,
      () => undefined,
    );
    return result;
  }, (request) => {
    writeLine({ measureText: request });
    return textMeasureResponse(lines.nextSync());
  });

  let propertyLayers: PropertyInputLayer[] = [];
  if (options.includes(PROPERTIES_STDIN)) {
    try {
      propertyLayers = await readPropertyInputs(lines);
    } catch (error) {
      writeLine({ error: describeError(error) });
      process.exitCode = 1;
      return;
    }
  }

  let defaultExport: EntryComponent;
  let prepare: (() => Promise<void>) | undefined;
  holdProjectPropertyValues();
  try {
    ({ defaultExport, prepare } = await loadEntry(entryPath, options.includes(SOURCE_LOCATIONS)));
  } catch (error) {
    writeLine({ error: describeError(error) });
    process.exitCode = 1;
    return;
  }

  // The schema exists only once the entry's module scope has run; checking
  // here reports every bad value before `prepare()` or any frame uses one.
  let properties: ReturnType<typeof resolvePropertyInputs>;
  try {
    properties = resolvePropertyInputs(propertyLayers, listProjectProperties(), path.dirname(entryPath));
  } catch (error) {
    writeLine({ error: describeError(error) });
    process.exitCode = 1;
    return;
  }
  if (properties.issues.length > 0) {
    writeLine({ invalidProperties: properties.issues });
    process.exitCode = 1;
    return;
  }
  setProjectPropertyValues(properties.values, properties.defaults);

  if (prepare) {
    // Runs once, before the persistent root is mounted and before any frame
    // is requested — the one point in this process's lifetime where async
    // work (e.g. fetching remote data to render with) can happen without
    // touching the otherwise fully synchronous mount/renderAt/request-loop
    // path. Whatever `prepare()` stashes into module-level state is then
    // read synchronously by every `renderAt()` call for the rest of this
    // process's life, so it is fetched once per export, not once per frame.
    try {
      await prepare();
    } catch (error) {
      writeLine({ error: describeError(error) });
      process.exitCode = 1;
      return;
    }
  }

  let mounted: MountedComposition;
  try {
    mounted = mount(defaultExport, compositionId);
  } catch (error) {
    writeLine({ error: describeError(error) });
    process.exitCode = 1;
    return;
  }

  writeLine({
    config: mounted.config,
    compositionId: mounted.compositionId,
    compositions: mounted.compositions,
    componentSchemas: listComponentSchemas(),
    propertySchema: listProjectProperties() ?? null,
    // Files `prepare()` read outside the sources the editor already watches.
    dependencies: prepareDependencies(),
  });

  let resolver: Resolver | null = null;
  // A JSX position's file name, relative to the entry, as the path to report.
  const resolveSourceFile = (fileName: string) => path.resolve(path.dirname(entryPath), fileName);

  while (true) {
    const next = await lines.next();
    if (next.done) {
      break;
    }
    const line = next.value;
    const trimmed = line.trim();
    if (trimmed.length === 0) {
      continue;
    }
    let request: Request;
    try {
      request = JSON.parse(trimmed);
    } catch (error) {
      writeLine({ error: `invalid request JSON: ${describeError(error)}` });
      continue;
    }
    try {
      if ('collectAudio' in request) {
        if (request.collectAudio !== true) throw new Error('collectAudio must be true');
        writeLine({ collectedAudio: mounted.collectAudio() });
      } else if ('collectSubtitles' in request) {
        const { from, frames } = request.collectSubtitles;
        if (!Number.isInteger(from) || !Number.isInteger(frames)) {
          throw new Error('collectSubtitles requires integer `from` and `frames`');
        }
        writeLine({ collectedSubtitles: mounted.collectSubtitles(from, frames) });
      } else if ('collectOccurrences' in request) {
        const { from, frames, preview } = request.collectOccurrences;
        if (!Number.isInteger(from) || !Number.isInteger(frames)) {
          throw new Error('collectOccurrences requires integer `from` and `frames`');
        }
        writeLine({
          collectedOccurrences: mounted.collectOccurrences(from, frames, resolveSourceFile, { preview: preview === true }),
        });
      } else if (isLayerSourceRequest(request)) {
        const report = mounted.layerSource(
          request.time,
          request.layerSource,
          resolveSourceFile,
          { preview: request.preview === true },
        );
        writeLine({ layerSource: report ?? { chain: null } });
      } else if (isResolveRequest(request)) {
        resolver ??= createResolver(mounted.config.lang);
        writeLine(resolver.resolve(request.components, request.runtime, mounted.fonts));
      } else {
        const openEnd = request.openEnd === true;
        // Compact frames leave out what Rust reads back by default, which
        // shrinks a NEBULA frame (1,500 particles) from 554 KiB to 458 KiB.
        const { scene, audio, subtitles, statics } = mounted.renderAt(request.time, request.project ?? null, {
          openEnd,
          compact: request.compactTransforms === true,
          statics: request.statics,
          debugGuides: request.debugGuides === true,
          preview: request.preview === true,
        });
        writeLine({
          scene,
          audio,
          ...(subtitles.length > 0 ? { subtitles } : {}),
          ...(statics
            ? {
              seq: statics.seq,
              ...(statics.defined.length > 0 ? { statics: statics.defined } : {}),
              ...(statics.dropped.length > 0 ? { droppedStatics: statics.dropped } : {}),
            }
            : {}),
          ...(openEnd ? { openEnd } : {}),
        });
      }
    } catch (error) {
      writeLine({ error: describeError(error) });
    }
  }
}

/** With this option the first stdin line carries `{ "propertyInputs": PropertyInputLayer[] }`. */
const PROPERTIES_STDIN = '--properties-stdin';
/**
 * With this option every message on stdout is a little-endian u32 byte count
 * followed by the JSON, as the Rust bridge reads it; without it, every
 * message is one JSON line, which is easier to read from scripts and tests.
 */
const LENGTH_PREFIXED = '--length-prefixed';
/**
 * With this option the entry is built with JSX positions, which `layerSource`
 * requests read. Only the editor's preview asks for it.
 */
const SOURCE_LOCATIONS = '--source-locations';

async function readPropertyInputs(lines: AsyncIterator<string>): Promise<PropertyInputLayer[]> {
  const next = await lines.next();
  if (next.done) {
    throw new Error('Celesta closed stdin before sending the project property inputs');
  }
  let message: { propertyInputs?: unknown };
  try {
    message = JSON.parse(next.value);
  } catch (error) {
    throw new Error(`invalid project property inputs: ${describeError(error)}`);
  }
  if (!Array.isArray(message.propertyInputs) || !message.propertyInputs.every(isPropertyInputLayer)) {
    throw new Error('invalid project property inputs: expected a propertyInputs array of { source, baseDir, values } layers');
  }
  return message.propertyInputs;
}

function isPropertyInputLayer(layer: unknown): layer is PropertyInputLayer {
  if (layer === null || typeof layer !== 'object') return false;
  const { source, baseDir, values, file } = layer as Record<string, unknown>;
  return (
    (source === 'project' || source === 'propsFile' || source === 'entryProps' || source === 'props') &&
    typeof baseDir === 'string' &&
    values !== null &&
    typeof values === 'object' &&
    !Array.isArray(values) &&
    (file === undefined || typeof file === 'string')
  );
}

async function requestMediaProbe(
  lines: AsyncIterator<string>,
  path: string,
): Promise<ProbedMediaInfo> {
  writeLine({ probeMedia: { path } });
  const next = await lines.next();
  if (next.done) {
    throw new Error('Celesta closed the media probe channel unexpectedly');
  }
  let response: ProbeMediaResponse;
  try {
    response = JSON.parse(next.value) as ProbeMediaResponse;
  } catch (error) {
    throw new Error(`invalid media probe response: ${describeError(error)}`);
  }
  if (response.error) {
    throw new Error(response.error);
  }
  if (!response.media) {
    throw new Error('Celesta returned an empty media probe response');
  }
  return response.media;
}

async function requestDecodedAudio(lines: AsyncIterator<string>, path: string): Promise<DecodedAudio> {
  writeLine({ decodeAudio: { path } });
  const next = await lines.next();
  if (next.done) {
    throw new Error('Celesta closed the audio decoding channel unexpectedly');
  }
  let response: DecodeAudioResponse;
  try {
    response = JSON.parse(next.value) as DecodeAudioResponse;
  } catch (error) {
    throw new Error(`invalid audio decoding response: ${describeError(error)}`);
  }
  if (response.error) {
    throw new Error(response.error);
  }
  if (!response.audio) {
    throw new Error('Celesta returned an empty audio decoding response');
  }
  // Read before the queue sends anything else: Celesta removes the file when
  // the next message arrives.
  const bytes = await fs.promises.readFile(response.audio.path);
  if (bytes.byteLength !== response.audio.frames * 4) {
    throw new Error(`decoded audio for ${path} is ${bytes.byteLength} bytes, expected ${response.audio.frames * 4}`);
  }
  const samples = new Float32Array(response.audio.frames);
  const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  for (let i = 0; i < samples.length; i += 1) {
    samples[i] = view.getFloat32(i * 4, true);
  }
  return { sampleRate: response.audio.sampleRate, samples };
}

async function requestTextMeasure(
  lines: AsyncIterator<string>,
  request: MeasureTextRequest,
): Promise<TextMetrics> {
  writeLine({ measureText: request });
  return textMeasureResponse(await lines.next());
}

function textMeasureResponse(next: IteratorResult<string>): TextMetrics {
  if (next.done) {
    throw new Error('Celesta closed the text measurement channel unexpectedly');
  }
  let response: { metrics?: TextMetrics; error?: string };
  try {
    response = JSON.parse(next.value);
  } catch (error) {
    throw new Error(`invalid text measurement response: ${describeError(error)}`);
  }
  if (response.error) {
    throw new Error(response.error);
  }
  if (!response.metrics) {
    throw new Error('Celesta returned an empty text measurement response');
  }
  return response.metrics;
}

let lengthPrefixed = false;
let messageBuffer = Buffer.allocUnsafe(64 * 1024);

function writeLine(value: Message): void {
  // Synchronous hooks may immediately wait for Rust's reply. Flush the whole
  // request without depending on Node's event loop to drain stdout.
  const json = JSON.stringify(value);
  let bytes: Buffer;
  if (lengthPrefixed) {
    // A little-endian u32 byte count, then the JSON, encoded straight into a
    // buffer reused across messages.
    const length = Buffer.byteLength(json);
    if (messageBuffer.length < length + 4) {
      messageBuffer = Buffer.allocUnsafe(Math.max(length + 4, messageBuffer.length * 2));
    }
    messageBuffer.writeUInt32LE(length, 0);
    messageBuffer.write(json, 4);
    bytes = messageBuffer.subarray(0, length + 4);
  } else {
    bytes = Buffer.from(`${json}\n`);
  }
  let offset = 0;
  while (offset < bytes.length) offset += fs.writeSync(1, bytes, offset, bytes.length - offset);
}

function describeError(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

/**
 * For an entry importing a name from a Celesta package that does not export
 * it, such as `Circle` from `@celesta/react`, an error naming the package
 * that does; undefined for any other error.
 */
function misplacedImportError(error: unknown): Error | undefined {
  const match = error instanceof SyntaxError
    ? /^Named export '([^']+)' not found\. The requested module '([^']+)'/.exec(error.message)
    : null;
  if (!match) return undefined;
  const [, name, requested] = match;
  const { dependencies } = require('../package.json') as { dependencies: Record<string, string> };
  let from: string | undefined;
  let provider: string | undefined;
  for (const pkg of Object.keys(dependencies).filter((dependency) => dependency.startsWith('@celesta/'))) {
    try {
      if (pathToFileURL(require.resolve(pkg)).href === requested) from = pkg;
      else if (name in (require(pkg) as object)) provider ??= pkg;
    } catch {
      // A package this Node.js cannot require (an ES module before require(esm)) is skipped.
    }
  }
  return from && provider
    ? new Error(`${from} does not export ${name}; import it from ${provider}`)
    : undefined;
}

interface LoadedEntry {
  defaultExport: EntryComponent;
  /** The entry's named `prepare` export, if it has one — see `main()`. */
  prepare: (() => Promise<void>) | undefined;
}

async function loadEntry(entryPath: string, sourceLocations: boolean): Promise<LoadedEntry> {
  const contents = path.resolve(__dirname, '../../..');
  if (process.platform === 'darwin' && path.basename(contents) === 'Contents') {
    // Keep native helper executables in the app's code directory for signing.
    process.env.ESBUILD_BINARY_PATH = path.join(contents, 'Helpers', 'esbuild');
  }
  // eslint-disable-next-line global-require -- optional, only needed by this CLI
  const esbuild = require('esbuild') as typeof import('esbuild');
  const result = await esbuild.build({
    entryPoints: [entryPath],
    bundle: true,
    write: false,
    platform: 'node',
    format: 'esm',
    jsx: 'automatic',
    // `jsxDEV(type, props, key, isStatic, { fileName, lineNumber, columnNumber })`
    // for every JSX element in the entry's own files.
    ...(sourceLocations ? { jsxDev: true } : {}),
    // `@celesta/shader` sources and subtitle files, imported as their text.
    loader: { '.wgsl': 'text', '.srt': 'text', '.vtt': 'text' },
    absWorkingDir: path.dirname(entryPath),
    logLevel: 'silent',
    define: {
      'import.meta.dirname': JSON.stringify(path.dirname(entryPath)),
      'import.meta.filename': JSON.stringify(entryPath),
    },
    // `react` must resolve to the exact module instance this process's own
    // reconciler is driving, or hooks fail with "Invalid hook call"
    // (the entry's own copy of React would look for a dispatcher the
    // reconciler never set on it). The Celesta packages' React Context
    // objects (CompositionRuntimeContext, ProjectLayersContext, ProjectContext,
    // ...) and registries (registerComponent(), the character host elements)
    // need the same treatment: they must be the exact objects this CLI's own
    // renderer reads. So every `@celesta/*` import stays external and resolves
    // from this CLI, which depends on all of them, through Node's own module
    // cache instead of being duplicated into the bundle or needing a project
    // install.
    plugins: [{
      name: 'shared-runtime',
      setup(build) {
        build.onResolve({ filter: /^(react(?:\/jsx(?:-dev)?-runtime)?|@celesta\/[^/]+(?:\/internal)?)$/ }, (args) => {
          let resolved: string;
          try {
            // React's dev runtime would need development React; Celesta's
            // records the positions on top of the production runtime.
            resolved = require.resolve(
              sourceLocations && args.path === 'react/jsx-dev-runtime' ? '@celesta/react/jsx-dev-runtime' : args.path,
            );
          } catch (error) {
            if ((error as NodeJS.ErrnoException).code !== 'MODULE_NOT_FOUND') throw error;
            return { errors: [{ text: `${args.path} is not a Celesta package` }] };
          }
          return { path: pathToFileURL(resolved).href, external: true };
        });
      },
    }],
  });
  const [output] = result.outputFiles;

  // Runtime imports are absolute file URLs, so installed packages stay read-only.
  const bundleDirectory = fs.mkdtempSync(path.join(tmpdir(), 'celesta-entry-'));
  const bundlePath = path.join(bundleDirectory, 'entry.mjs');
  let mod: unknown;
  try {
    fs.writeFileSync(bundlePath, output.text);
    mod = await import(pathToFileURL(bundlePath).href);
  } catch (error) {
    throw misplacedImportError(error) ?? error;
  } finally {
    fs.rmSync(bundleDirectory, { recursive: true, force: true });
  }

  const defaultExport =
    mod !== null && typeof mod === 'object' && 'default' in mod
      ? (mod as { default: unknown }).default
      : mod;
  if (typeof defaultExport !== 'function') {
    throw new Error('the entry module must have a default export that is a React component');
  }
  const prepareExport = mod !== null && typeof mod === 'object' ? (mod as { prepare?: unknown }).prepare : undefined;
  if (prepareExport !== undefined && typeof prepareExport !== 'function') {
    throw new Error("the entry module's `prepare` export, if present, must be a function");
  }
  return {
    defaultExport: defaultExport as EntryComponent,
    prepare: prepareExport as (() => Promise<void>) | undefined,
  };
}

/** A shared buffer prevents async reads from swallowing synchronous RPC replies. */
class ProtocolLines implements AsyncIterator<string> {
  private pending = Buffer.alloc(0);
  private ended = false;

  private takeLine(): IteratorResult<string> | undefined {
    const newline = this.pending.indexOf(10);
    if (newline >= 0 || (this.ended && this.pending.length > 0)) {
      const end = newline >= 0 ? newline : this.pending.length;
      const value = this.pending.subarray(0, end).toString('utf8');
      this.pending = this.pending.subarray(end + 1);
      return { done: false, value };
    }
    return this.ended ? { done: true, value: undefined } : undefined;
  }

  async next(): Promise<IteratorResult<string>> {
    while (true) {
      const line = this.takeLine();
      if (line) return line;
      const chunk = Buffer.alloc(4096);
      const count = await new Promise<number>((resolve, reject) => {
        fs.read(0, chunk, 0, chunk.length, null, (error, bytesRead) => {
          if (error) reject(error);
          else resolve(bytesRead);
        });
      });
      this.ended = count === 0;
      this.pending = Buffer.concat([this.pending, chunk.subarray(0, count)]);
    }
  }

  nextSync(): IteratorResult<string> {
    while (true) {
      const line = this.takeLine();
      if (line) return line;
      const chunk = Buffer.alloc(4096);
      const count = fs.readSync(0, chunk, 0, chunk.length, null);
      this.ended = count === 0;
      this.pending = Buffer.concat([this.pending, chunk.subarray(0, count)]);
    }
  }
}

main();
