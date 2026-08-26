import * as fs from 'node:fs';
import * as path from 'node:path';
import * as readline from 'node:readline';

import { createResolver, mount } from './render';
import type {
  AudioClipDescriptor,
  ComponentResolution,
  ComponentResolutionRequest,
  EntryComponent,
  MountedComposition,
  ProjectFrame,
  Resolver,
} from './render';
import { listComponentSchemas } from './registry';
import type { ComponentPropertySchema } from './registry';
import type { CompositionConfig, Scene, Time } from './scene';

interface FrameRequest {
  time: Time;
  project?: ProjectFrame;
}

interface ResolveRequest {
  components: ComponentResolutionRequest[];
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

  let defaultExport: EntryComponent;
  try {
    defaultExport = await loadEntryDefault(path.resolve(entry));
  } catch (error) {
    writeLine({ error: describeError(error) });
    process.exitCode = 1;
    return;
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
        writeLine({ components: resolver.resolve(request.components) });
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

async function loadEntryDefault(entryPath: string): Promise<EntryComponent> {
  // eslint-disable-next-line global-require -- optional, only needed by this CLI
  const esbuild = require('esbuild') as typeof import('esbuild');
  const result = await esbuild.build({
    entryPoints: [entryPath],
    bundle: true,
    write: false,
    platform: 'node',
    format: 'cjs',
    jsx: 'automatic',
    absWorkingDir: path.dirname(entryPath),
    logLevel: 'silent',
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

  // The bundle keeps `require('@mikan/react')` external, which is only
  // resolvable from inside this package's own directory tree (Node's
  // self-reference resolution), so the bundle is written there rather than
  // to the OS temp directory.
  const tempRoot = path.join(__dirname, '..', '.tmp');
  fs.mkdirSync(tempRoot, { recursive: true });
  const bundleDirectory = fs.mkdtempSync(path.join(tempRoot, 'entry-'));
  const bundlePath = path.join(bundleDirectory, 'entry.cjs');
  fs.writeFileSync(bundlePath, output.text);
  let mod: unknown;
  try {
    // eslint-disable-next-line global-require, import/no-dynamic-require -- entry path is only known at runtime
    mod = require(bundlePath);
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
  return defaultExport as EntryComponent;
}

main();
