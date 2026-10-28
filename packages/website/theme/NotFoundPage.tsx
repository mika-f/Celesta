import { RouteLink, getHomeRoute, getLocalesData, getSiteData } from '@natsuneko-laboratory/makit-runtime';
import type { NotFoundPageProps } from '@natsuneko-laboratory/makit-runtime';
import { ArrowRight } from 'lucide-react';
import type { ReactNode } from 'react';
import { copy, type Locale } from '../site/copy/index.ts';
import { DocumentTitle } from './DocumentTitle.tsx';
import { LocaleProvider } from './LocaleContext.tsx';
import { LocaleLinks } from './LocaleLinks.tsx';
import { locales } from './locale.ts';

/** One locale's copy of a region; CSS shows the variant matching `data-nr-locale` (set in `RootLayout`). */
const Variants = ({ render }: { render: (locale: Locale) => ReactNode }) => locales.map((locale) =>
  <div key={locale} lang={locale} data-nr-variant={locale}><LocaleProvider locale={locale}>{render(locale)}</LocaleProvider></div>);

/**
 * Makit exports a single `404.html` for every URL, so the page cannot know its locale at build
 * time. Every locale renders, and the one named by the URL's first segment is shown.
 */
export async function NotFoundPage({ components: C }: NotFoundPageProps) {
  const [site, i18n] = await Promise.all([getSiteData(), getLocalesData()]);
  const homes = Object.fromEntries(await Promise.all(locales.map(async (locale) =>
    [locale, (await getHomeRoute(locale)) ?? `${site.basePath}/${locale}/`] as const))) as Record<Locale, string>;
  const localeLinks = (current: Locale) => locales.map((locale) =>
    ({ locale, label: i18n.locales.find((candidate) => candidate.urlLocale === locale)?.label ?? locale, href: homes[locale], current: locale === current }));

  const titles = Object.fromEntries(locales.map((locale) => [locale, copy(locale).notFound.title]));
  // The first `<title>` wins; the script fixes it before paint, `DocumentTitle` after hydration.
  const titleScript = `document.title=${JSON.stringify(titles).replace(/</g, '\\u003c')}[document.documentElement.getAttribute('data-nr-locale')]||document.title`;

  return <div className="nr-page">
    <title>{titles[locales[0]]}</title>
    <script dangerouslySetInnerHTML={{ __html: titleScript }} />
    <DocumentTitle titles={titles} />
    <Variants render={(locale) => <C.Header
      header={site.header}
      siteTitle={site.title}
      homeHref={homes[locale]}
      actions={<>
        <LocaleLinks links={localeLinks(locale)} label={copy(locale).common.navigation.language} />
        {site.theme.colorScheme === 'system' && <C.ThemeToggle />}
      </>}
    />} />
    <main id="main" className="nr-main nr-not-found" tabIndex={-1}>
      <Variants render={(locale) => {
        const { notFound } = copy(locale);
        return <div className="nr-shell">
          <p className="nr-eyebrow" aria-hidden="true">404 · --:--:--:--</p>
          <h1>{notFound.heading}</h1>
          <p className="nr-not-found-description">{notFound.description}</p>
          <RouteLink className="nr-button" href={homes[locale]}>{notFound.back}<ArrowRight size={16} strokeWidth={1.75} aria-hidden="true" /></RouteLink>
          <div className="nr-not-found-rail" aria-hidden="true">
            <div className="nr-rail"><i className="nr-rail-clip" /><i className="nr-rail-clip" /><i className="nr-rail-clip" /><span className="nr-rail-head" /></div>
            <div className="nr-rail-tc"><span>00:00</span><b>--:--:--:--</b><span>00:10</span></div>
          </div>
        </div>;
      }} />
    </main>
    <Variants render={() => <C.Footer footer={site.footer} />} />
  </div>;
}
