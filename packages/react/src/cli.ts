import { Console } from 'node:console';
import * as fs from 'node:fs';
import { tmpdir } from 'node:os';
import { createRequire } from 'node:module';
import * as path from 'node:path';
import { pathToFileURL } from 'node:url';

import { createResolver, mount } from './render';
import type {
  AudioClipDescriptor,
  ComponentResolution,
  ComponentResolutionRequest,
  EntryComponent,
  MountedComposition,
  ProjectFrame,
  ResolutionRuntime,
  Resolver,
} from './render';
import { listComponentSchemas } from './registry';
import type { ComponentPropertySchema } from './registry';
import { holdProjectPropertyValues, listProjectProperties, setProjectPropertyValues } from './properties';
import type { ProjectPropertyField } from './properties';
import { resolvePropertyInputs } from './property-inputs';
import type { PropertyInputLayer, PropertyIssue } from './property-inputs';
import { setMediaProbe } from './media';
import { setTextMeasurer } from './text-measure';
import type { MeasureTextRequest, TextMetrics } from './text-measure';
import type { ProbedMediaInfo } from './media';
import type { CompositionConfig, EvaluatedTransform, Layer, LayerContent, Scene, Time } from './scene';

// stdout is the JSON request/response channel the Rust bridge
// (crates/react-bridge) parses one line at a time; anything else written
// there corrupts the protocol and fails the export. Route every `console`
// method to stderr — which the bridge inherits straight to the user's
// terminal — so a `console.log` left in a composition is evaluated and
// printed to the console instead of breaking the run.
globalThis.console = new Console({ stdout: process.stderr, stderr: process.stderr });

interface FrameRequest {
  time: Time;
  project?: ProjectFrame;
  /** Sent by bridges that fill omitted transform fields with their defaults. */
  compactTransforms?: boolean;
}

/** A layer whose transform leaves out fields equal to the Rust defaults. */
type CompactLayer = Omit<Layer, 'transform' | 'content'> & {
  transform: Partial<EvaluatedTransform>;
  content:
    | Exclude<LayerContent, { type: 'group' }>
    | (Omit<Extract<LayerContent, { type: 'group' }>, 'layers'> & { layers: CompactLayer[] });
};

interface ResolveRequest {
  components: ComponentResolutionRequest[];
  /** Absent from older bridges; resolved components then see frame 0. */
  runtime?: ResolutionRuntime;
}

type Request = FrameRequest | ResolveRequest | { collectAudio: true };

interface ProbeMediaResponse {
  media?: ProbedMediaInfo;
  error?: string;
}

function isResolveRequest(request: Request): request is ResolveRequest {
  return Array.isArray((request as ResolveRequest).components);
}

async function main(): Promise<void> {
  const [entry, ...options] = process.argv.slice(2);
  if (!entry || options.some((option) => option !== PROPERTIES_STDIN)) {
    process.stderr.write(`usage: celesta-react-render <entry-file> [${PROPERTIES_STDIN}]\n`);
    process.exitCode = 1;
    return;
  }

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
    ({ defaultExport, prepare } = await loadEntry(entryPath));
  } catch (error) {
    writeLine({ error: describeError(error) });
    process.exitCode = 1;
    return;
  }

  // The schema exists only once the entry's module scope has run; checking
  // here reports every bad value before `prepare()` or any frame uses one.
  const properties = resolvePropertyInputs(propertyLayers, listProjectProperties(), path.dirname(entryPath));
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
    mounted = mount(defaultExport);
  } catch (error) {
    writeLine({ error: describeError(error) });
    process.exitCode = 1;
    return;
  }

  writeLine({
    config: mounted.config,
    componentSchemas: listComponentSchemas(),
    propertySchema: listProjectProperties() ?? null,
  });

  let resolver: Resolver | null = null;

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
      } else if (isResolveRequest(request)) {
        resolver ??= createResolver(mounted.config.lang);
        writeLine({ components: resolver.resolve(request.components, request.runtime, mounted.fonts) });
      } else {
        const { scene, audio } = mounted.renderAt(request.time, request.project ?? null);
        writeLine(
          request.compactTransforms
            ? { scene: { ...scene, layers: compactLayers(scene.layers) }, audio }
            : { scene, audio },
        );
      }
    } catch (error) {
      writeLine({ error: describeError(error) });
    }
  }
}

/** With this option the first stdin line carries `{ "propertyInputs": PropertyInputLayer[] }`. */
const PROPERTIES_STDIN = '--properties-stdin';

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
  if (!Array.isArray(message.propertyInputs)) {
    throw new Error('invalid project property inputs: expected a propertyInputs array');
  }
  return message.propertyInputs as PropertyInputLayer[];
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

function writeLine(
  value:
    | {
        config: CompositionConfig;
        componentSchemas: Record<string, ComponentPropertySchema>;
        propertySchema: Record<string, ProjectPropertyField> | null;
      }
    | { scene: Scene; audio: AudioClipDescriptor[] }
    | { scene: Omit<Scene, 'layers'> & { layers: CompactLayer[] }; audio: AudioClipDescriptor[] }
    | { collectedAudio: AudioClipDescriptor[] }
    | { components: ComponentResolution[] }
    | { probeMedia: { path: string } }
    | { measureText: MeasureTextRequest }
    | { invalidProperties: PropertyIssue[] }
    | { error: string },
): void {
  // Synchronous hooks may immediately wait for Rust's reply. Flush the whole
  // request without depending on Node's event loop to drain stdout.
  const bytes = Buffer.from(`${JSON.stringify(value)}\n`);
  let offset = 0;
  while (offset < bytes.length) offset += fs.writeSync(1, bytes, offset, bytes.length - offset);
}

// Matches `EvaluatedTransform::default()` in crates/composition. Most layers
// keep identity scale and rotation; dropping them shrinks a NEBULA frame
// (1,500 particles) from 554 KiB to 458 KiB.
function compactLayers(layers: Layer[]): CompactLayer[] {
  return layers.map((layer) => {
    const { position, scale, rotation, anchor } = layer.transform;
    const transform: Partial<EvaluatedTransform> = {};
    if (position.x !== 0 || position.y !== 0) transform.position = position;
    if (scale.x !== 1 || scale.y !== 1) transform.scale = scale;
    if (rotation !== 0) transform.rotation = rotation;
    if (anchor.x !== 0.5 || anchor.y !== 0.5) transform.anchor = anchor;
    const { content } = layer;
    return {
      ...layer,
      transform,
      content: content.type === 'group' ? { ...content, layers: compactLayers(content.layers) } : content,
    };
  });
}

function describeError(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

interface LoadedEntry {
  defaultExport: EntryComponent;
  /** The entry's named `prepare` export, if it has one — see `main()`. */
  prepare: (() => Promise<void>) | undefined;
}

async function loadEntry(entryPath: string): Promise<LoadedEntry> {
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
    absWorkingDir: path.dirname(entryPath),
    logLevel: 'silent',
    define: {
      'import.meta.dirname': JSON.stringify(path.dirname(entryPath)),
      'import.meta.filename': JSON.stringify(entryPath),
    },
    // `react` must resolve to the exact module instance this process's own
    // react-reconciler is driving, or hooks fail with "Invalid hook call"
    // (the entry's own copy of React would look for a dispatcher the
    // reconciler never set on it). `@celesta/react`'s React Context objects
    // (CompositionRuntimeContext, ProjectLayersContext,
    // ProjectTrackLayersContext, ProjectContext) need
    // the same treatment: they must be the exact object identity the
    // entry's `useCurrentFrame()`/`useProject()`/etc. read from, matching
    // the Provider values render.ts sets around it. Both stay external and
    // resolve through Node's own module cache instead of being duplicated
    // into the bundle. `@celesta/math` is stateless, but the entry's own
    // directory has no copy of it either, so it resolves the same way.
    plugins: [{
      name: 'shared-runtime',
      setup(build) {
        build.onResolve({ filter: /^@celesta\/(?:code|voicevox)$/ }, (args) => {
          let packagePath: string;
          try {
            packagePath = require.resolve(args.path);
          } catch (error) {
            if ((error as NodeJS.ErrnoException).code !== 'MODULE_NOT_FOUND') throw error;
            // Source builds keep optional packages beside React.
            const name = args.path.slice('@celesta/'.length);
            packagePath = createRequire(path.join(__dirname, `../../${name}/package.json`)).resolve(args.path);
          }
          // Bundle optional packages, so their Celesta imports share the externals
          // below instead of loading peer copies or requiring a project install.
          return { path: packagePath };
        });
        build.onResolve({ filter: /^(react(?:\/jsx(?:-dev)?-runtime)?|@celesta\/(?:react|math))$/ }, (args) => ({
          path: pathToFileURL(require.resolve(args.path)).href,
          external: true,
        }));
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
