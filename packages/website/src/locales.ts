/** Register a language here, then add its complete catalog in catalog.ts. */
export const locales = { en: 'English', ja: '日本語' } as const;
export type Locale = keyof typeof locales;
export const localeCodes = Object.keys(locales) as Locale[];
export const defaultLocale: Locale = 'en';

export function localeFromPath(path: string): Locale {
  const candidate = path.split('/')[1];
  return localeCodes.find(locale => locale === candidate) ?? defaultLocale;
}

/** Respect browser preference order and quality values, including regional tags. */
export function preferredLocale(acceptLanguage: string | null): Locale {
  const preferences = (acceptLanguage ?? '').split(',').map(value => {
    const [tag, ...parameters] = value.trim().toLowerCase().split(';');
    const quality = parameters.find(parameter => parameter.trim().startsWith('q='));
    const weight = quality ? Number(quality.trim().slice(2)) : 1;
    return { tag, weight };
  }).filter(({ weight }) => Number.isFinite(weight) && weight > 0 && weight <= 1)
    .sort((a, b) => b.weight - a.weight);
  for (const { tag } of preferences) {
    const match = localeCodes.find(locale => locale === tag || locale === tag.split('-')[0]);
    if (match) return match;
    if (tag === '*') return defaultLocale;
  }
  return defaultLocale;
}

export function languageRedirect(url: URL, acceptLanguage: string | null): URL | null {
  if (url.pathname !== '/' && url.pathname !== '/index.html') return null;
  const target = new URL(url);
  target.pathname = `/${preferredLocale(acceptLanguage)}/`;
  return target;
}
