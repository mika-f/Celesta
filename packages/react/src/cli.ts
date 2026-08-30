import { Console } from 'node:console';
import * as fs from 'node:fs';
import * as path from 'node:path';
import * as readline from 'node:readline';
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
import { listProjectProperties } from './properties';
import type { ProjectPropertyField } from './properties';
import type { CompositionConfig, Scene, Time } from './scene';

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
}

interface ResolveRequest {
  components: ComponentResolutionRequest[];
  /** Absent from older bridges; resolved components then see frame 0. */
  runtime?: ResolutionRuntime;
}

type Request = FrameRequest | ResolveRequest;

function isResolveRequest(request: Request): request is ResolveRequest {
  return Array.isArray((request as ResolveRequest).components);
}

async function main(): Promise<void> {
  const entry = process.argv[2];
  if (!entry) {
    process.stderr.write('usage: mikan-react-render <entry-file>\n');
    process.exitCode = 1;
    return;
  }

  const entryPath = path.resolve(entry);
  // Local-file helpers used during `prepare()` (loadLipSync, loadPsdPreset)
  // resolve relative paths against this, matching how the Rust renderer
  // resolves a relative `<Audio>`/`<Image>` src against the entry directory.
  process.env.MIKAN_REACT_ENTRY_DIR = path.dirname(entryPath);

  let defaultExport: EntryComponent;
  let prepare: (() => Promise<void>) | undefined;
  try {
    ({ defaultExport, prepare } = await loadEntry(entryPath));
  } catch (error) {
    writeLine({ error: describeError(error) });
    process.exitCode = 1;
    return;
  }

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

  const rl = readline.createInterface({ input: process.stdin, terminal: false });
  for await (const line of rl) {
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
      if (isResolveRequest(request)) {
        resolver ??= createResolver();
        writeLine({ components: resolver.resolve(request.components, request.runtime) });
      } else {
        const { scene, audio } = mounted.renderAt(request.time, request.project ?? null);
        writeLine({ scene, audio });
      }
    } catch (error) {
      writeLine({ error: describeError(error) });
    }
  }
}

function writeLine(
  value:
    | {
        config: CompositionConfig;
        componentSchemas: Record<string, ComponentPropertySchema>;
        propertySchema: Record<string, ProjectPropertyField> | null;
      }
    | { scene: Scene; audio: AudioClipDescriptor[] }
    | { components: ComponentResolution[] }
    | { error: string },
): void {
  process.stdout.write(`${JSON.stringify(value)}\n`);
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
    // reconciler never set on it). `@mikan/react`'s React Context objects
    // (CompositionRuntimeContext, ProjectLayersContext,
    // ProjectTrackLayersContext, ProjectContext) need
    // the same treatment: they must be the exact object identity the
    // entry's `useCurrentFrame()`/`useProject()`/etc. read from, matching
    // the Provider values render.ts sets around it. Both stay external and
    // resolve through Node's own module cache instead of being duplicated
    // into the bundle.
    external: ['react', 'react/jsx-runtime', 'react/jsx-dev-runtime', '@mikan/react'],
  });
  const [output] = result.outputFiles;

  // The bundle keeps `@mikan/react` external, which is only
  // resolvable from inside this package's own directory tree (Node's
  // self-reference resolution), so the bundle is written there rather than
  // to the OS temp directory.
  const tempRoot = path.join(__dirname, '..', '.tmp');
  fs.mkdirSync(tempRoot, { recursive: true });
  const bundleDirectory = fs.mkdtempSync(path.join(tempRoot, 'entry-'));
  const bundlePath = path.join(bundleDirectory, 'entry.mjs');
  fs.writeFileSync(bundlePath, output.text);
  let mod: unknown;
  try {
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

main();
