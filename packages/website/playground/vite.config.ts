import { copyFileSync, cpSync, mkdirSync, rmSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { defineConfig, type Manifest, type Plugin } from 'vite';
import react from '@vitejs/plugin-react';
import { showcase, showcaseAssetDirectories, showcaseExtraImages } from '../site/showcase-catalog.ts';
import { sampleSources } from './sample-sources.ts';

/** The repository root: examples are read from here. */
const repoRoot = fileURLToPath(new URL('../../..', import.meta.url));
/** packages/website: the island writes into its `public/`, which Makit serves and copies to `dist/`. */
const site = fileURLToPath(new URL('..', import.meta.url));
const publicDir = join(site, 'public');
const outDir = join(publicDir, '_playground');
const base = '/_playground/';

/**
 * After each build: `manifest.json` names the hashed entry and its CSS for `PlaygroundMount`, and the
 * showcase's repository files are copied beside it (`/showcase-assets/…`, posters at `/showcase/<slug>.jpg`).
 */
function islandOutput(): Plugin {
  return {
    name: 'celesta-playground-island',
    writeBundle(_, bundle) {
      const asset = bundle['.vite/manifest.json'];
      if (!asset || asset.type !== 'asset') throw new Error('The playground build did not emit .vite/manifest.json.');
      const manifest = JSON.parse(String(asset.source)) as Manifest;
      const entry = Object.values(manifest).find(chunk => chunk.isEntry);
      if (!entry) throw new Error('The playground manifest has no entry.');
      writeFileSync(join(outDir, 'manifest.json'), JSON.stringify({ entry: base + entry.file, css: (entry.css ?? []).map(name => base + name) }, null, 2) + '\n');
      // Vite's own manifest is only read here; keep it out of the deployed site.
      rmSync(join(outDir, '.vite'), { recursive: true, force: true });

      const assets = join(publicDir, 'showcase-assets');
      rmSync(assets, { recursive: true, force: true });
      for (const path of showcaseAssetDirectories) cpSync(join(repoRoot, path), join(assets, path), { recursive: true });
      const posters = join(publicDir, 'showcase');
      rmSync(posters, { recursive: true, force: true });
      mkdirSync(posters, { recursive: true });
      // Only the catalog's works with `poster: true`; a missing poster fails the build.
      for (const work of showcase) {
        if ('poster' in work && work.poster) copyFileSync(join(repoRoot, 'examples', work.slug, 'poster.jpg'), join(posters, `${work.slug}.jpg`));
      }
      for (const extra of showcaseExtraImages) {
        copyFileSync(join(repoRoot, extra.source), join(posters, extra.dest));
      }
    },
  };
}

export default defineConfig({
  root: site,
  base,
  // `public/` is Makit's; the island writes into it, so Vite must not copy it again.
  publicDir: false,
  plugins: [sampleSources(repoRoot), react(), islandOutput()],
  build: {
    // An app build with a module entry, not `build.lib`: library mode always inlines assets as
    // data URLs (Vite's `shouldInline`), which would put the 19 MB composition worker in the entry.
    rolldownOptions: {
      input: fileURLToPath(new URL('main.ts', import.meta.url)),
      preserveEntrySignatures: 'exports-only',
      // Every hashed file under assets/, so one immutable rule in public/_headers covers them and spares manifest.json.
      output: { entryFileNames: 'assets/playground-[hash].js' },
    },
    outDir,
    emptyOutDir: true,
    manifest: true,
    target: 'es2022',
    chunkSizeWarningLimit: 25_000,
  },
  worker: { format: 'es' },
});
