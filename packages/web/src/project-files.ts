import type { Plugin } from 'esbuild-wasm';

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
        if (['react', '@celesta/react', '@celesta/math', '@celesta/code'].includes(args.path)) return { path: args.path, external: true };
        if (args.kind !== 'entry-point' && !args.path.startsWith('.')) return { errors: [{ text: `Import ${JSON.stringify(args.path)} is unavailable in the web editor.` }] };
        const path = normalize(args.kind === 'entry-point' ? entry : `${args.importer.slice(0, args.importer.lastIndexOf('/') + 1)}${args.path}`);
        const resolved = [path, `${path}.tsx`, `${path}.ts`, `${path}.jsx`, `${path}.js`, `${path}.json`, `${path}/index.tsx`, `${path}/index.ts`].find(candidate => Object.hasOwn(files, candidate));
        return resolved ? { path: resolved, namespace: 'composition' } : { errors: [{ text: `Missing project source: ${path}` }] };
      });
      build.onLoad({ filter: /.*/, namespace: 'composition' }, args => ({
        contents: files[args.path], loader: args.path.endsWith('.json') ? 'json' : 'tsx',
      }));
    },
  };
}
