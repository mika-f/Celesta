import { createElement, Fragment, isValidElement, type ReactNode } from 'react';
import { Trans } from 'react-i18next';
import { i18n, translate, type Message } from './catalog.ts';
import { localeFromPath } from './locales.ts';

export const currentLocale = localeFromPath(typeof window === 'undefined' ? '/en/' : window.location.pathname);
export const homePath = `/${currentLocale}/`;
export const t = (key: Message, values?: Record<string, string | number>) => translate(currentLocale, key, values);

export function text(key: Message, components: ReactNode[] = []): ReactNode {
  return richText(t(key), components);
}

/** Named Trans slots allow nested elements and preserve copy when a wrapper is missing. */
export function richText(message: string, components: ReactNode[] = []): ReactNode {
  const slots = Object.fromEntries([...message.matchAll(/<slot(\d+)\/?>/g)].map(([, index]) => {
    const component = components[Number(index)];
    return [`slot${index}`, isValidElement(component) ? component : createElement(Fragment, null, component)];
  }));
  return createElement(Trans, { i18n, defaults: message, components: slots, shouldUnescape: true });
}
