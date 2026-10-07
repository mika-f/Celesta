import { showcase } from './showcase-catalog';

const sources = import.meta.glob('../../../examples/**/*.{ts,tsx}', { query: '?raw', import: 'default' });
export const posters = import.meta.glob('../../../examples/*/poster.jpg', { query: '?url', import: 'default', eager: true }) as Record<string, string>;

export async function loadShowcase(slug: string): Promise<Record<string, string>> {
  if (!showcase.some(sample => sample.slug === slug && !('desktop' in sample))) throw new Error('This composition requires the desktop app.');
  const prefix = `../../../examples/${slug}/`;
  const files = await Promise.all(Object.entries(sources).filter(([path]) => path.startsWith(prefix)).map(async ([path, load]) => [path.slice(prefix.length), await load()] as [string, string]));
  if (!files.some(([path]) => path === 'film.tsx')) throw new Error('This composition has no browser source files.');
  return Object.fromEntries(files);
}
