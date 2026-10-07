import { cloneElement, isValidElement, type ReactNode } from 'react';
import { translate, type Message } from './catalog.ts';
import { localeFromPath } from './locales.ts';

export const currentLocale = localeFromPath(typeof window === 'undefined' ? '/en/' : window.location.pathname);
export const homePath = `/${currentLocale}/`;
export const t = (key: Message, values?: Record<string, string | number>) => translate(currentLocale, key, values);

/** Numbered placeholders preserve React elements, code and links without HTML injection. */
export function text(key: Message, components: ReactNode[] = []): ReactNode {
  return richText(t(key), components);
}

/** Preserve translated copy even when a caller omits its wrapper. */
export function richText(message: string, components: ReactNode[] = []): ReactNode {
  const tokens = message.split(/(<\/?\d+\/?>)/g);
  let cursor = 0;
  function read(): ReactNode[] {
    const children: ReactNode[] = [];
    while (cursor < tokens.length) {
      const token = tokens[cursor++];
      if (/^<\/\d+>$/.test(token)) break;
      const match = /^<(\d+)(\/?)>$/.exec(token);
      if (!match) { children.push(token.replace(/&lt;/g, '<').replace(/&gt;/g, '>').replace(/&amp;/g, '&')); continue; }
      const component = components[Number(match[1])];
      if (match[2]) {
        children.push(isValidElement(component) ? cloneElement(component, { key: cursor }) : component);
      } else {
        const nested = read();
        if (isValidElement<{ children?: ReactNode }>(component)) children.push(cloneElement(component, { key: cursor, children: nested }));
        else children.push(...nested);
      }
    }
    return children;
  }
  return read();
}
