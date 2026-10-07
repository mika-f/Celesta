export const showcase = [
  { slug: 'reel', title: 'Reel', duration: '32 sec · 30 FPS', frame: 75 },
  { slug: 'apex', title: 'Apex', duration: '60 sec · 30 FPS', frame: 150 },
  { slug: 'afterimage', title: 'Afterimage', duration: '24 sec · 30 FPS', frame: 100 },
  { slug: 'signal', title: 'Signal', duration: '16 sec · 30 FPS', frame: 175 },
  { slug: 'spectra', title: 'Spectra', duration: '11.5 sec · 60 FPS', frame: 60 },
  { slug: '36-days', title: '36 Days', duration: '44 sec · 30 FPS', frame: 150 },
  { slug: 'prism', title: 'Prism', duration: '48 sec · 30 FPS', desktop: true },
  { slug: 'feature-tour', title: 'Feature Tour', duration: '52 sec · 30 FPS', desktop: true },
  { slug: 'versus', title: 'Versus', duration: '82 sec · 30 FPS', desktop: true },
] as const;

// Keep these paths pure so Vite can use the same catalog for static pages.
export const showcasePath = (slug = '', locale = 'en') => `/${locale}/showcase/${slug ? `${slug}/` : ''}`;
