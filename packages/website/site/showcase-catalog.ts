/** `poster: true` marks works with an `examples/<slug>/poster.jpg`, which the playground build copies to `/showcase/<slug>.jpg`. */
export const showcase = [
  { slug: 'reel', title: 'Reel', duration: '32 sec · 30 FPS', frame: 75 },
  { slug: 'apex', title: 'Apex', duration: '60 sec · 30 FPS', poster: true, frame: 150 },
  { slug: 'afterimage', title: 'Afterimage', duration: '24 sec · 30 FPS', poster: true, frame: 100 },
  { slug: 'signal', title: 'Signal', duration: '16 sec · 30 FPS', poster: true, frame: 175 },
  { slug: 'spectra', title: 'Spectra', duration: '11.5 sec · 60 FPS', frame: 60 },
  { slug: '36-days', title: '36 Days', duration: '44 sec · 30 FPS', poster: true, frame: 150 },
  { slug: 'prism', title: 'Prism', duration: '48 sec · 30 FPS', poster: true, desktop: true },
  { slug: 'feature-tour', title: 'Feature Tour', duration: '52 sec · 30 FPS', poster: true, desktop: true },
  { slug: 'versus', title: 'Versus', duration: '82 sec · 30 FPS', desktop: true },
] as const;

/** Additional repository images copied to `/showcase/<dest>` for landing page use-case cards. */
export const showcaseExtraImages = [
  { source: 'examples/assets/dialogue-demo/poster.jpg', dest: 'dialogue-demo.jpg' },
] as const;

/** Repository directories the showcase reads at `/showcase-assets/<directory>/<file>`; the playground build copies them. */
export const showcaseAssetDirectories = ['examples/afterimage/assets/fonts', 'examples/versus/bench/assets/fonts', 'examples/versus/bench/assets/licenses', 'examples/assets/dialogue-demo/portraits'];
