'use strict';

const fs = require('node:fs');
const path = require('node:path');
const readline = require('node:readline');

const { readConfig, renderSceneAt } = require('./render');

async function main() {
  const entry = process.argv[2];
  if (!entry) {
    process.stderr.write('usage: mikan-react-render <entry-file>\n');
    process.exitCode = 1;
    return;
  }

  let defaultExport;
  try {
    defaultExport = await loadEntryDefault(path.resolve(entry));
  } catch (error) {
    writeLine({ error: describeError(error) });
    process.exitCode = 1;
    return;
  }

  let config;
  try {
    config = readConfig(defaultExport);
  } catch (error) {
    writeLine({ error: describeError(error) });
    process.exitCode = 1;
    return;
  }

  writeLine({ config });

  const rl = readline.createInterface({ input: process.stdin, terminal: false });
  for await (const line of rl) {
    const trimmed = line.trim();
    if (trimmed.length === 0) {
      continue;
    }
    let request;
    try {
      request = JSON.parse(trimmed);
    } catch (error) {
      writeLine({ error: `invalid request JSON: ${describeError(error)}` });
      continue;
    }
    try {
      const scene = renderSceneAt(defaultExport, request.time);
      writeLine({ scene });
    } catch (error) {
      writeLine({ error: describeError(error) });
    }
  }
}

function writeLine(value) {
  process.stdout.write(`${JSON.stringify(value)}\n`);
}

function describeError(error) {
  return error instanceof Error ? error.message : String(error);
}

async function loadEntryDefault(entryPath) {
  // eslint-disable-next-line global-require -- optional, only needed by this CLI
  const esbuild = require('esbuild');
  const result = await esbuild.build({
    entryPoints: [entryPath],
    bundle: true,
    write: false,
    platform: 'node',
    format: 'cjs',
    jsx: 'automatic',
    absWorkingDir: path.dirname(entryPath),
    logLevel: 'silent',
    // `@mikan/react`'s component markers are matched by object identity in
    // render.js. Bundling the package would duplicate those functions, so it
    // must stay external and resolve through Node's own module cache instead.
    external: ['@mikan/react'],
  });
  const [output] = result.outputFiles;

  // The bundle keeps `require('@mikan/react')` external so its component
  // markers resolve to the very functions render.js compares by identity,
  // instead of a second copy baked into the bundle. That require call is
  // only resolvable from inside this package's own directory tree (Node's
  // self-reference resolution), so the bundle is written there rather than
  // to the OS temp directory.
  const tempRoot = path.join(__dirname, '..', '.tmp');
  fs.mkdirSync(tempRoot, { recursive: true });
  const bundleDirectory = fs.mkdtempSync(path.join(tempRoot, 'entry-'));
  const bundlePath = path.join(bundleDirectory, 'entry.cjs');
  fs.writeFileSync(bundlePath, output.text);
  let mod;
  try {
    // eslint-disable-next-line global-require, import/no-dynamic-require -- entry path is only known at runtime
    mod = require(bundlePath);
  } finally {
    fs.rmSync(bundleDirectory, { recursive: true, force: true });
  }

  const defaultExport = mod && typeof mod === 'object' && 'default' in mod ? mod.default : mod;
  if (typeof defaultExport !== 'function') {
    throw new Error('the entry module must have a default export that is a React component');
  }
  return defaultExport;
}

main();
