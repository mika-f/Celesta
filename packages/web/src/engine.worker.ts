import * as esbuild from 'esbuild-wasm';
import wasmURL from 'esbuild-wasm/esbuild.wasm?url';
import * as math from '../../math/src/index';
import * as celesta from '../../react/src/browser';
import type { MountedComposition } from '../../react/src/render';

let initialized: Promise<void> | undefined;
let mounted: MountedComposition | null = null;

self.onmessage = async (event: MessageEvent<{ id: number; type: 'compile' | 'frame'; source?: string; frame?: number }>) => {
  const { id, type } = event.data;
  try {
    if (type === 'compile') {
      initialized ??= esbuild.initialize({ wasmURL, worker: false });
      await initialized;
      const { code } = await esbuild.transform(event.data.source ?? '', {
        loader: 'tsx', format: 'cjs', target: 'es2022',
        jsx: 'transform', jsxFactory: 'React.createElement', jsxFragment: 'React.Fragment',
        sourcefile: 'composition.tsx',
      });
      const module: { exports: Record<string, unknown> } = { exports: {} };
      const require = (name: string): unknown => {
        if (name === '@celesta/react') return celesta;
        if (name === '@celesta/math') return math;
        if (name === 'react') return celesta.React;
        throw new Error(`Import ${JSON.stringify(name)} is unavailable in the web editor. Use @celesta/react, @celesta/math, and react.`);
      };
      // Visitor-authored code runs in a dedicated worker, away from the page DOM.
      new Function('module', 'exports', 'require', 'React', code)(module, module.exports, require, celesta.React);
      if (typeof module.exports.default !== 'function') {
        throw new Error('Export a default React component that returns <Composition>.');
      }
      mounted = celesta.mount(module.exports.default as celesta.EntryComponent);
      self.postMessage({ id, value: mounted.config });
    } else {
      if (!mounted) throw new Error('Compile a composition first.');
      const frame = event.data.frame ?? 0;
      const { frameRate } = mounted.config;
      const value = mounted.renderAt({ value: frame * frameRate.denominator, timescale: frameRate.numerator }, null);
      self.postMessage({ id, value });
    }
  } catch (cause) {
    self.postMessage({ id, error: cause instanceof Error ? cause.message : String(cause) });
  }
};
