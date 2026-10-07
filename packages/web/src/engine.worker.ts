import * as esbuild from 'esbuild-wasm';
import wasmURL from 'esbuild-wasm/esbuild.wasm?url';
import * as math from '../../math/src/index';
import * as celesta from '../../react/src/browser';
import * as codeComponents from '../../code/src/index';
import { setTextMeasurer } from '../../react/src/text-measure';
import type { CompileOptions } from './engine';
import { projectFiles } from './project-files';
import { loadFonts } from './fonts';
import { textMeasurer } from './text-layout';
import type { MountedComposition } from '../../react/src/render';

let initialized: Promise<void> | undefined;
let mounted: MountedComposition | null = null;
let options: CompileOptions = {};

self.onmessage = async (event: MessageEvent<{ id: number; type: 'compile' | 'frame'; source?: string; frame?: number; options?: CompileOptions }>) => {
  const { id, type } = event.data;
  try {
    if (type === 'compile') {
      initialized ??= esbuild.initialize({ wasmURL, worker: false });
      await initialized;
      options = event.data.options ?? {};
      const source = event.data.source ?? '';
      const entry = options.entry ?? 'composition.tsx';
      const transformOptions = {
        loader: 'tsx', format: 'cjs', target: 'es2022',
        jsx: 'transform', jsxFactory: 'React.createElement', jsxFragment: 'React.Fragment',
        sourcefile: entry,
      } as const;
      const code = options.files
        ? (await esbuild.build({
          entryPoints: [entry], bundle: true, write: false, format: 'cjs', target: 'es2022',
          jsxFactory: 'React.createElement', jsxFragment: 'React.Fragment',
          plugins: [projectFiles({ ...options.files, [entry]: source }, entry)],
        })).outputFiles![0].text
        : (await esbuild.transform(source, transformOptions)).code;
      const module: { exports: Record<string, unknown> } = { exports: {} };
      const require = (name: string): unknown => {
        if (name === '@celesta/react') return celesta;
        if (name === '@celesta/math') return math;
        if (name === '@celesta/code') return codeComponents;
        if (name === 'react') return celesta.React;
        throw new Error(`Import ${JSON.stringify(name)} is unavailable in the web editor. Use @celesta/react, @celesta/math, @celesta/code, and react.`);
      };
      // Visitor-authored code runs in a dedicated worker, away from the page DOM.
      new Function('module', 'exports', 'require', 'React', code)(module, module.exports, require, celesta.React);
      if (typeof module.exports.default !== 'function') {
        throw new Error('Export a default React component that returns <Composition>.');
      }
      const resolveFont = (asset: { location: { type: 'file'; path: string } | { type: 'url'; url: string } }) => {
        if (asset.location.type === 'url') return asset.location.url;
        if (!options.baseURL) throw new Error(`Missing font file: ${asset.location.path}. Use a font URL or provide baseURL.`);
        return new URL(asset.location.path, options.baseURL).href;
      };
      const measure = textMeasurer();
      setTextMeasurer(async request => { await loadFonts(request.fonts, resolveFont); return measure(request); }, measure);
      const entryComponent = module.exports.default as celesta.EntryComponent;
      if (typeof module.exports.prepare === 'function') await module.exports.prepare();
      mounted = celesta.mount(entryComponent);
      await loadFonts([...mounted.fonts], resolveFont);
      mounted = celesta.mount(entryComponent);
      self.postMessage({ id, value: mounted.config });
    } else {
      if (!mounted) throw new Error('Compile a composition first.');
      const frame = event.data.frame ?? 0;
      const { frameRate } = mounted.config;
      const value = mounted.renderAt({ value: frame * frameRate.denominator, timescale: frameRate.numerator }, null);
      if (options.silent) value.audio = [];
      self.postMessage({ id, value });
    }
  } catch (cause) {
    self.postMessage({ id, error: cause instanceof Error ? cause.message : String(cause) });
  }
};
