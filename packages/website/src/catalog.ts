import { createInstance, type ParseKeys } from 'i18next';
import en from './locales/en.json' with { type: 'json' };
import ja from './locales/ja.json' with { type: 'json' };
import { defaultLocale, localeCodes, type Locale } from './locales.ts';

export const catalogs = { en, ja } satisfies Record<Locale, typeof en>;

declare module 'i18next' {
  interface CustomTypeOptions {
    defaultNS: 'translation';
    resources: { translation: typeof en };
    returnNull: false;
  }
}

export type Message = ParseKeys<'translation'>;

// Bundled resources initialize synchronously in both Vite and the browser.
export const i18n = createInstance();
void i18n.init({
  initAsync: false,
  lng: defaultLocale,
  fallbackLng: defaultLocale,
  supportedLngs: localeCodes,
  resources: Object.fromEntries(localeCodes.map(locale => [locale, { translation: catalogs[locale] }])),
  returnNull: false,
  interpolation: { escapeValue: false }, // React and the HTML builder escape the output.
  react: { transSupportBasicHtmlNodes: false },
});

export function translate(locale: Locale, key: Message, values: Record<string, string | number> = {}): string {
  return i18n.getFixedT(locale)(key, values);
}
