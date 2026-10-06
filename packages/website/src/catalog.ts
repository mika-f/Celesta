import en from './locales/en.json' with { type: 'json' };
import ja from './locales/ja.json' with { type: 'json' };
import type { Locale } from './locales.ts';

export type Message = keyof typeof en;
export const catalogs = { en, ja } satisfies Record<Locale, Record<Message, string>>;

export function translate(locale: Locale, key: Message, values: Record<string, string | number> = {}): string {
  return catalogs[locale][key].replace(/\{(\w+)\}/g, (placeholder, name: string) => String(values[name] ?? placeholder));
}
