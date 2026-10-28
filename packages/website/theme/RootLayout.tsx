import type { RootLayoutProps } from '@natsuneko-laboratory/makit-runtime';
import { copy } from '../site/copy/index.ts';
import { locales } from './locale.ts';

/**
 * Sets `<html lang>` and `data-nr-locale` from the URL before first paint, on routes that belong
 * to no locale (the 404, `/`), whose layout Makit renders without one.
 */
const localeScript = `(function(){var l=${JSON.stringify(locales)},s=location.pathname.split('/')[1],d=document.documentElement;s=l.indexOf(s)<0?l[0]:s;d.lang=s;d.setAttribute('data-nr-locale',s)})()`;

/**
 * The document shell (Makit THEME §10.3): `htmlProps` and `bodyProps` spread once, `bodyStart`
 * first in `<body>`. The skip link carries every locale's label; CSS shows the one in
 * `data-nr-locale`, because the 404 is rendered once for every locale.
 */
export function RootLayout({ locale, htmlProps, bodyProps, bodyStart, children }: RootLayoutProps) {
  return <html {...htmlProps} data-nr-locale={locale?.urlLocale}>
    <body {...bodyProps}>
      {bodyStart}
      {/* Makit has no icon or theme-color option; React hoists these into `<head>`. */}
      <link rel="icon" href="/favicon.svg" type="image/svg+xml" />
      <meta name="theme-color" media="(prefers-color-scheme: light)" content="#fafafa" />
      <meta name="theme-color" media="(prefers-color-scheme: dark)" content="#000000" />
      {!locale && <script dangerouslySetInnerHTML={{ __html: localeScript }} />}
      <a className="nr-skip" href="#main">
        {locales.map((variant) => <span key={variant} lang={variant} data-nr-variant={variant}>{copy(variant).home.skipToContent}</span>)}
      </a>
      {children}
    </body>
  </html>;
}
