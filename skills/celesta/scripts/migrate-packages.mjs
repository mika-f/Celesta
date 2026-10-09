#!/usr/bin/env node
// Moves imports of APIs that left `@celesta/react` to the package that now
// provides them, for code written before Celesta was split into packages:
//
//   import { Circle, Composition, TextBox } from '@celesta/react';
// becomes
//   import { Composition } from '@celesta/react';
//   import { Circle } from '@celesta/shapes';
//   import { TextBox } from '@celesta/text';
//
//   node migrate-packages.mjs [paths...] [--dry-run | --check]
//
// Paths are files or directories (default: the current directory); directories
// are searched for .ts/.tsx/.js/.jsx/.mts/.cts/.mjs/.cjs files, skipping
// node_modules, dist, .celesta, and .git. Named imports and `export { … } from`
// re-exports are rewritten, including `import type` and inline `type`
// specifiers, aliases, and multi-line lists; a moved name joins an existing
// import from its new package. `import * as C from '@celesta/react'` cannot be
// rewritten safely, so the moved names it uses are reported instead. A
// package.json that lists `@celesta/react` gets the new packages with the same
// version range. --dry-run reports without writing; --check also exits 1 when
// anything would change. Needs only Node.js.

import { readFileSync, readdirSync, statSync, writeFileSync, existsSync } from 'node:fs';
import * as path from 'node:path';
import { fileURLToPath } from 'node:url';

/** Every name `@celesta/react` exported before the split that now lives elsewhere. */
export const movedExports = {
  '@celesta/shapes': [
    'Arrow', 'ArrowProps', 'Circle', 'CircleProps', 'Ellipse', 'EllipseProps', 'Line', 'LineProps', 'Path',
    'PathProps', 'Polyline', 'PolylinePoint', 'PolylineProps', 'pointOnPolyline',
  ],
  '@celesta/layout': [
    'Camera', 'CameraProps', 'Center', 'CenterProps', 'Fit', 'FitProps', 'Grid', 'GridProps', 'Insets', 'SafeArea',
    'SafeAreaProps', 'Stack', 'StackProps', 'useLayoutBounds',
  ],
  '@celesta/transitions': [
    'SlideFrom', 'Transition', 'TransitionDirection', 'TransitionProps', 'TransitionSeries', 'TransitionSeriesEdge',
    'TransitionSeriesItem', 'TransitionSeriesProps', 'TransitionSeriesScene', 'TransitionSeriesSceneItem',
    'TransitionSeriesSceneTransition', 'TransitionSeriesSequenceProps', 'TransitionSeriesTiming',
    'TransitionSeriesTransitionItem', 'TransitionSeriesTransitionProps', 'TransitionSeriesType', 'TransitionType',
    'computeTransitionSeries', 'useTransitionSeriesScene', 'useTransitionVolume',
  ],
  '@celesta/text': [
    'CountUpOptions', 'FitTextOptions', 'FitTextResult', 'FitTextStyle', 'TextBox', 'TextBoxProps', 'TextReveal',
    'TextRevealProps', 'Typewriter', 'TypewriterOptions', 'fitText', 'useCountUp', 'useFitText', 'useTypewriter',
  ],
  '@celesta/debug': ['DebugBounds', 'DebugBoundsProps', 'DebugOverlay', 'DebugOverlayProps'],
  '@celesta/media-utils': ['MediaAudioInfo', 'MediaInfo', 'MediaVideoInfo', 'mediaDurationInFrames', 'preloadMedia'],
  '@celesta/character': [
    'BlinkPhase', 'BlinkTiming', 'Character', 'CharacterLipSync', 'CharacterPortrait', 'CharacterProps',
    'CharacterSubtitle', 'CharacterView', 'CharacterViewProps', 'CharacterViewReference', 'Dialogue', 'DialogueLine',
    'DialoguePlan', 'DialogueProps', 'DialogueRange', 'DialogueScene', 'DialogueSeries', 'DialogueSeriesProps',
    'ImageCharacterBlink', 'ImageCharacterPortrait', 'LipSyncOptions', 'LipSyncTrack', 'LoadPsdPresetOptions',
    'MouthKeyframe', 'MouthShape', 'ParsedPfv', 'PfvFavorite', 'PlanDialogueOptions', 'PlannedDialogueLine',
    'PsdCharacterBlink', 'PsdCharacterLipSync', 'PsdCharacterPortrait', 'PsdExpression', 'SubtitleCharacter',
    'SubtitleRenderProps', 'WavAudio', 'blinkPhase', 'buildEnvelope', 'decodeWav', 'lipSyncFromKeyframes',
    'lipSyncTimeline', 'loadLipSync', 'loadPsdPreset', 'parsePfv', 'planDialogue', 'resolveVisibleLayers',
    'useLipSync', 'vowelShapes',
  ],
  '@celesta/project': [
    'Asset', 'AssetSource', 'CharacterDefinition', 'LipSyncCue', 'LipSyncDefinition', 'PortraitDefinition',
    'Project', 'ProjectProvider', 'ProjectProviderProps', 'ProjectSettings', 'ProjectTimeline', 'ProjectTrack',
    'ProjectTrackProps', 'ProjectVersion', 'SourceRange', 'SubtitleDefinition', 'TimelineContent', 'TimelineItem',
    'Track', 'TrackKind', 'loadProject', 'loadProjectFromString', 'useProject', 'useProjectProperty',
    'useProjectTrack',
  ],
};

const CORE = '@celesta/react';
const PACKAGE_ORDER = [CORE, ...Object.keys(movedExports)];
const NEW_HOME = new Map(Object.entries(movedExports).flatMap(([pkg, names]) => names.map((name) => [name, pkg])));
const DECLARATION = /\b(import|export)(\s+type)?\s*\{([^}]*)\}\s*from\s*(['"])(@celesta\/[a-z-]+)\4(\s*;)?/g;
const NAMESPACE = /\bimport\s+\*\s+as\s+([A-Za-z_$][\w$]*)\s+from\s*(['"])@celesta\/react\2/g;
const LINE_WIDTH = 100;

/** A specifier's imported name: `type Foo as Bar` → `Foo`. */
function importedName(spec) {
  return spec.replace(/^type\s+/, '').split(/\s+as\s+/)[0].trim();
}

function lineOf(text, offset) {
  return text.slice(0, offset).split('\n').length;
}

/**
 * Rewrites one file's source. Returns the new text, the packages its moved
 * names now come from, and warnings for what it could not rewrite.
 */
export function migrateSource(text) {
  const declarations = [];
  for (const match of text.matchAll(DECLARATION)) {
    const [whole, keyword, typeKeyword = '', body, quote, pkg, semi = ''] = match;
    const lineStart = text.lastIndexOf('\n', match.index - 1) + 1;
    const before = text.slice(lineStart, match.index);
    declarations.push({
      start: match.index,
      end: match.index + whole.length,
      whole,
      keyword,
      typeKeyword: typeKeyword ? 'type ' : '',
      specs: body.split(',').map((spec) => spec.trim()).filter(Boolean),
      quote,
      pkg,
      semi: semi.trim(),
      multiline: body.includes('\n'),
      indent: /^\s*$/.test(before) ? before : null,
      innerIndent: body.match(/\n([ \t]*)\S/)?.[1] ?? '  ',
      added: [],
      emitted: [],
    });
  }

  const packages = new Set();
  const warnings = [];
  let changed = false;
  for (const declaration of declarations.filter((d) => d.pkg === CORE)) {
    const stay = [];
    const moving = new Map();
    for (const spec of declaration.specs) {
      const pkg = NEW_HOME.get(importedName(spec));
      if (!pkg) {
        stay.push(spec);
        continue;
      }
      if (!moving.has(pkg)) moving.set(pkg, []);
      moving.get(pkg).push(spec);
      packages.add(pkg);
    }
    if (moving.size === 0) continue;
    changed = true;
    declaration.specs = stay;
    for (const pkg of PACKAGE_ORDER.filter((name) => moving.has(name))) {
      // Join an import of the same kind from that package, if the file has one.
      const target = declaration.keyword === 'import' && declarations.find((other) =>
        other.pkg === pkg && other.keyword === 'import' && other.typeKeyword === declaration.typeKeyword);
      if (target) target.added.push(...moving.get(pkg));
      else declaration.emitted.push({ ...declaration, pkg, specs: moving.get(pkg) });
    }
  }

  for (const match of text.matchAll(NAMESPACE)) {
    const namespace = match[1];
    const used = [...new Set([...text.matchAll(new RegExp(`\\b${namespace.replace(/\$/g, '\\$')}\\.([A-Za-z_$][\\w$]*)`, 'g'))]
      .map((use) => use[1]))].filter((name) => NEW_HOME.has(name));
    if (used.length > 0) {
      warnings.push(`line ${lineOf(text, match.index)}: \`import * as ${namespace}\` uses moved names; import them from their packages: ` +
        used.map((name) => `${namespace}.${name} (${NEW_HOME.get(name)})`).join(', '));
    }
  }
  if (!changed) return { text, packages, warnings };

  const render = (declaration) => {
    const from = `from ${declaration.quote}${declaration.pkg}${declaration.quote}`;
    const head = `${declaration.keyword} ${declaration.typeKeyword}`;
    const single = `${head}{ ${declaration.specs.join(', ')} } ${from}`;
    if (!declaration.multiline || (declaration.indent ?? '').length + single.length <= LINE_WIDTH) return single;
    const lines = declaration.specs.map((spec) => `${declaration.innerIndent}${spec},`).join('\n');
    return `${head}{\n${lines}\n${declaration.indent ?? ''}} ${from}`;
  };

  let output = '';
  let cursor = 0;
  for (const declaration of declarations) {
    const existing = new Set(declaration.specs);
    declaration.specs.push(...declaration.added.filter((spec) => !existing.has(spec)));
    if (declaration.pkg !== CORE && declaration.added.length === 0) continue;
    const parts = [...(declaration.specs.length > 0 ? [declaration] : []), ...declaration.emitted].map(render);
    let start = declaration.start;
    let end = declaration.end;
    let replacement;
    if (parts.length === 0) {
      // Nothing is left of it: drop the declaration and, on its own line, the line.
      replacement = '';
      if (declaration.indent !== null && text[end] === '\n') {
        start -= declaration.indent.length;
        end += 1;
      } else if (text[end] === ' ') {
        end += 1;
      }
    } else if (declaration.indent !== null) {
      const semi = declaration.semi;
      replacement = parts.join(`${semi}\n${declaration.indent}`) + semi;
    } else {
      // Inside a line (a string of source, say): keep the extra declarations on it.
      replacement = parts.join('; ') + declaration.semi;
    }
    output += text.slice(cursor, start) + replacement;
    cursor = end;
  }
  output += text.slice(cursor);
  return { text: output, packages, warnings };
}

const EXTENSIONS = new Set(['.ts', '.tsx', '.js', '.jsx', '.mts', '.cts', '.mjs', '.cjs']);
const SKIPPED = new Set(['node_modules', 'dist', '.celesta', '.git']);

function* sourceFiles(target) {
  const stat = statSync(target);
  if (stat.isFile()) {
    yield target;
    return;
  }
  for (const entry of readdirSync(target, { withFileTypes: true })) {
    if (entry.isDirectory()) {
      if (!SKIPPED.has(entry.name)) yield* sourceFiles(path.join(target, entry.name));
    } else if (entry.isFile() && EXTENSIONS.has(path.extname(entry.name))) {
      yield path.join(target, entry.name);
    }
  }
}

function nearestManifest(file) {
  for (let dir = path.dirname(path.resolve(file)); ; dir = path.dirname(dir)) {
    const manifest = path.join(dir, 'package.json');
    if (existsSync(manifest)) return manifest;
    if (path.dirname(dir) === dir) return null;
  }
}

/** Adds `packages` beside `@celesta/react` in whichever dependency list has it. Returns what it added. */
export function addDependencies(manifestText, packages) {
  const manifest = JSON.parse(manifestText);
  const added = [];
  for (const field of ['dependencies', 'devDependencies', 'peerDependencies']) {
    const list = manifest[field];
    if (!list || !(CORE in list)) continue;
    const missing = [...packages].filter((pkg) => !(pkg in list)).sort();
    if (missing.length === 0) continue;
    const next = {};
    for (const [name, range] of Object.entries(list)) {
      next[name] = range;
      if (name === CORE) for (const pkg of missing) next[pkg] = range;
    }
    manifest[field] = next;
    added.push(...missing);
  }
  if (added.length === 0) return { text: manifestText, added };
  const indent = manifestText.match(/\n([ \t]+)"/)?.[1] ?? '  ';
  return { text: `${JSON.stringify(manifest, null, indent)}\n`, added };
}

function main(argv) {
  const flags = new Set(argv.filter((arg) => arg.startsWith('--')));
  for (const flag of flags) {
    if (flag !== '--dry-run' && flag !== '--check') {
      console.error(`unknown option ${flag}\nusage: node migrate-packages.mjs [paths...] [--dry-run | --check]`);
      return 2;
    }
  }
  const write = flags.size === 0;
  const targets = argv.filter((arg) => !arg.startsWith('--'));
  const manifests = new Map();
  let changedFiles = 0;
  for (const target of targets.length > 0 ? targets : ['.']) {
    for (const file of sourceFiles(target)) {
      const source = readFileSync(file, 'utf8');
      if (!source.includes(CORE)) continue;
      const result = migrateSource(source);
      for (const warning of result.warnings) console.warn(`${file}:${warning.replace(/^line /, '')}`);
      if (result.text === source) continue;
      changedFiles += 1;
      console.log(`${write ? 'updated' : 'would update'} ${file} (${[...result.packages].join(', ')})`);
      if (write) writeFileSync(file, result.text);
      const manifest = nearestManifest(file);
      if (manifest) {
        if (!manifests.has(manifest)) manifests.set(manifest, new Set());
        for (const pkg of result.packages) manifests.get(manifest).add(pkg);
      }
    }
  }
  for (const [manifest, packages] of manifests) {
    const before = readFileSync(manifest, 'utf8');
    const { text, added } = addDependencies(before, packages);
    if (added.length === 0) continue;
    console.log(`${write ? 'added' : 'would add'} ${added.join(', ')} to ${manifest}`);
    if (write) writeFileSync(manifest, text);
  }
  console.log(changedFiles === 0 ? 'Nothing to migrate.' : `${changedFiles} file(s) ${write ? 'migrated' : 'to migrate'}.`);
  return flags.has('--check') && changedFiles > 0 ? 1 : 0;
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  process.exitCode = main(process.argv.slice(2));
}
