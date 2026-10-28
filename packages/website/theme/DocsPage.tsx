import { getHomeRoute, RouteLink } from '@natsuneko-laboratory/makit-runtime';
import type { DocsPageProps, GeneratedPage } from '@natsuneko-laboratory/makit-runtime';
import { ArrowRight, ChevronRight } from 'lucide-react';
import { copy, isLocale, type Locale } from '../site/copy/index.ts';
import { HeaderActions } from './HeaderActions.tsx';
import { LocaleProvider } from './LocaleContext.tsx';

/** `Docs › Group › Page` — the page's ancestors, separated by chevrons; the docs root reads "Docs". */
function Trail({ page, docsRoot, locale }: { page: GeneratedPage; docsRoot: string; locale: Locale }) {
  const { common, theme } = copy(locale);
  const label = (title: string, href?: string) => (href === docsRoot ? common.navigation.docs : title);
  const items = page.hierarchy.filter((node) => node.title || node.href).map((node) => ({ title: label(node.title, node.href), href: node.href }));
  if (items.at(-1)?.href !== page.route) items.push({ title: label(page.title, page.route), href: page.route });
  return <nav className="nr-trail" aria-label={theme.docs.breadcrumb}>
    <ol>
      {items.map((item, index) => <li key={index}>
        {index > 0 && <ChevronRight size={14} aria-hidden="true" />}
        {index === items.length - 1
          ? <span aria-current="page">{item.title}</span>
          : item.href ? <RouteLink href={item.href}>{item.title}</RouteLink> : <span>{item.title}</span>}
      </li>)}
    </ol>
  </nav>;
}

/** "Try it without installing" — sends readers to the playground on the home page. */
function PlaygroundCta({ locale }: { locale: Locale }) {
  const { docsCta } = copy(locale);
  return <section className="nr-docs-cta" aria-labelledby="nr-docs-cta">
    <h2 id="nr-docs-cta">{docsCta.tryItWithoutInstalling(children => <em>{children}</em>)}</h2>
    <p>{docsCta.thePlaygroundRunsCelestaInYourBrowser}</p>
    <RouteLink className="nr-button" href={`/${locale}/#playground`}>{docsCta.openThePlayground(() => <ArrowRight size={16} strokeWidth={1.75} aria-hidden="true" />)}</RouteLink>
  </section>;
}

/**
 * Sidebar · article · table of contents from 1100 px, sidebar · article from 860 px, one column
 * with the sidebar as a drawer below. `LocaleProvider` tells the Footer which locale to render and gives the
 * table of contents its labels.
 */
export async function DocsPage({ page, site, i18n, navigation, components: C }: DocsPageProps) {
  const locale = isLocale(page.locale) ? page.locale : 'en';
  // Until the locale has a home page, link the brand to `/<locale>/` so the header keeps its locale.
  const homeHref = (await getHomeRoute(page.locale)) ?? `${site.basePath}/${page.locale}/`;
  const { minDepth, maxDepth } = site.markdown.tableOfContents;
  const sidebar = page.sidebar && navigation.length > 0;
  const toc = page.tableOfContents && page.headings.some((heading) => heading.id && heading.depth >= minDepth && heading.depth <= maxDepth);

  return <LocaleProvider locale={locale} docs={copy(locale).theme.docs}>
    <div className="nr-page">
      <C.Header header={site.header} siteTitle={site.title} homeHref={homeHref} actions={<HeaderActions page={page} site={site} i18n={i18n} components={C} />} />
      <div className="nr-shell nr-docs" data-sidebar={sidebar ? '' : undefined} data-toc={toc ? '' : undefined}>
        {sidebar && <C.Sidebar navigation={navigation} currentRoute={page.route} components={C} />}
        <main id="main" className="nr-docs-main" tabIndex={-1}>
          <C.FallbackNotice page={page} i18n={i18n} />
          <Trail page={page} docsRoot={`${site.basePath}/${page.locale}/docs/`} locale={locale} />
          <article data-pagefind-body>
            <C.PageHeader page={page} />
            <C.PageContent html={page.html} copyButton={site.markdown.code.copyButton} />
          </article>
          <C.PrevNextLinks prev={page.navigationPosition?.prev} next={page.navigationPosition?.next} />
          <PlaygroundCta locale={locale} />
        </main>
        {toc && <div className="nr-docs-toc">
          <C.TableOfContents
            headings={page.headings}
            minDepth={minDepth}
            maxDepth={maxDepth}
            actions={<C.PageActions
              route={page.route}
              editUrl={page.isFallback ? undefined : page.editUrl}
              markdownEnabled={site.llms.enabled && !page.isFallback && !page.draft}
              placement="table-of-contents"
            />}
          />
        </div>}
      </div>
      <C.Footer footer={site.footer} />
    </div>
  </LocaleProvider>;
}
