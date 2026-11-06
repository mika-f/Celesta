import type { Plugin } from 'esbuild-wasm';
import { currentPackageName } from '../../react/src/package-scope';

/** The packages the web editor provides to compositions. */
export const runtimeModules = [
  'react',
  '@celesta.studio/react',
  '@celesta.studio/math',
  '@celesta.studio/shapes',
  '@celesta.studio/lottie',
  '@celesta.studio/layout',
  '@celesta.studio/components',
  '@celesta.studio/transitions',
  '@celesta.studio/text',
  '@celesta.studio/debug',
  '@celesta.studio/media-utils',
  '@celesta.studio/character',
  '@celesta.studio/code',
  '@celesta.studio/shader',
] as const;

/** Virtual project files; no filesystem or network module resolution. */
export function projectFiles(files: Record<string, string>, entry: string): Plugin {
  const normalize = (path: string) => {
    const parts: string[] = [];
    for (const part of path.split('/')) {
      if (part === '..') parts.pop();
      else if (part && part !== '.') parts.push(part);
    }
    return parts.join('/');
  };
  return {
    name: 'composition-files',
    setup(build) {
      build.onResolve({ filter: /.*/ }, args => {
        if ((runtimeModules as readonly string[]).includes(currentPackageName(args.path))) return { path: args.path, external: true };
        if (args.kind !== 'entry-point' && !args.path.startsWith('.')) return { errors: [{ text: `Import ${JSON.stringify(args.path)} is unavailable in the web editor.` }] };
        const path = normalize(args.kind === 'entry-point' ? entry : `${args.importer.slice(0, args.importer.lastIndexOf('/') + 1)}${args.path}`);
        const extensions = ['tsx', 'ts', 'jsx', 'js', 'json'];
        const resolved = [path, ...extensions.map(ext => `${path}.${ext}`), ...extensions.map(ext => `${path}/index.${ext}`)].find(candidate => Object.hasOwn(files, candidate));
        return resolved ? { path: resolved, namespace: 'composition' } : { errors: [{ text: `Missing project source: ${path}` }] };
      });
      build.onLoad({ filter: /.*/, namespace: 'composition' }, args => ({
        contents: files[args.path],
        loader: args.path.endsWith('.json') ? 'json' : /\.(?:srt|vtt)$/.test(args.path) ? 'text' : 'tsx',
      }));
    },
  };
}
