import { showcasePath } from './showcase-catalog';
import { LanguageSwitcher } from './LanguageSwitcher';
import { currentLocale, t, text, homePath } from './i18n';
import { useEffect, useState } from 'react';
import { Brand } from './Brand';
import { contents, repository } from './docs-content';
import { docGroups, docPages, docPath } from './docs-nav';
import { packageContents } from './docs-packages';

const pageContents = { ...contents, ...packageContents };

const overviewTopic = { slug: 'overview', title: t('docs.ui.overview'), description: t('docs.overview.overview-description'), keywords: t('docs.overview.keywords') };

/** The page for the current address: `/docs/` is the overview, `/docs/<slug>/` a chapter. */
function currentSlug(): string {
  const slug = window.location.pathname.replace(/^\/(?:[^/]+\/)?docs\/?/, '').replace(/\/+$/, '');
  return docPages.some(page => page.slug === slug) ? slug : 'overview';
}

export function Docs() {
  const [query, setQuery] = useState('');
  const [contentsOpen, setContentsOpen] = useState(false);
  const slug = currentSlug();
  const index = docPages.findIndex(page => page.slug === slug);
  const page = docPages[index];
  const group = docGroups.find(candidate => candidate.pages.includes(page));
  const previous = docPages[index - 1];
  const next = docPages[index + 1];
  const normalizedQuery = query.trim().toLowerCase();
  const results = [overviewTopic, ...docPages].filter(topic => `${topic.title} ${topic.description} ${topic.keywords}`.toLowerCase().includes(normalizedQuery));

  useEffect(() => {
    // The guide used to be one page with `/docs/#<chapter>` anchors. Keep those links working.
    const legacy = window.location.hash.slice(1);
    if (slug === 'overview' && docPages.some(topic => topic.slug === legacy)) window.location.replace(docPath(legacy));
  }, [slug]);

  useEffect(() => {
    if (page) document.title = t('docs.metadata.title', { title: page.title });
    // The page's content is mounted by React, after the HTML is loaded.
    const frame = requestAnimationFrame(() => {
      if (window.location.hash) document.getElementById(window.location.hash.slice(1))?.scrollIntoView({ behavior: 'instant' });
    });
    return () => cancelAnimationFrame(frame);
  }, [page]);

  const topicLink = (topic: { slug: string; title: string }) => <a key={topic.slug} href={docPath(topic.slug)} aria-current={slug === topic.slug ? 'page' : undefined}>{topic.title}</a>;

  return <div className="docs-page">
    <a className="skip-link" href="#docs-main">{text('docs.ui.skip-to-documentation')}</a>
    <header className="header docs-header">
      <div className="shell header-inner flex items-center justify-between">
        <div className="flex items-center gap-5">{text('docs.ui.documentation', [<Brand />, <a className="docs-header-label" href={docPath('overview')} />])}</div>
        <a className="docs-showcase-link" href={showcasePath('', currentLocale)}>{t('showcase.label')}</a><LanguageSwitcher /><nav className="docs-top-nav flex items-center gap-6" aria-label={t('docs.ui.main-navigation')}>{text('docs.ui.website-github', [<a href={homePath} />, <a href={repository} />, <span aria-hidden="true" />])}</nav>
      </div>
    </header>
    <div className="docs-layout shell">
      <aside className="docs-sidebar">
        <button className="docs-contents-toggle" aria-expanded={contentsOpen} aria-controls="docs-contents" onClick={() => setContentsOpen(!contentsOpen)}>{page ? page.title : t('docs.ui.documentation-topics')} <span aria-hidden="true">{contentsOpen ? '−' : '+'}</span></button>
        <div id="docs-contents" className={`docs-contents ${contentsOpen ? 'is-open' : ''}`}>
          <label className="docs-search-label" htmlFor="topic-search">{text('docs.ui.find-a-topic')}</label>
          <div className="docs-search"><span aria-hidden="true">⌕</span><input id="topic-search" type="search" placeholder={t('docs.ui.setup-math-export')} value={query} onChange={event => setQuery(event.target.value)} />{query && <button onClick={() => setQuery('')} aria-label={t('docs.ui.clear-topic-search')}>×</button>}</div>
          <nav aria-label={t('docs.ui.documentation-topics')}>
            <a href={showcasePath('', currentLocale)}>{t('showcase.label')} ↗</a>
            {normalizedQuery
              ? results.map(topicLink)
              : <>
                <a href={docPath('overview')} aria-current={slug === 'overview' ? 'page' : undefined}>{text('docs.ui.overview')}</a>
                {docGroups.map(section => <div key={section.title} className="docs-nav-group">
                  <p className="docs-nav-label">{section.title}</p>
                  {section.pages.map(topicLink)}
                </div>)}
              </>}
          </nav>
          <p className="docs-search-status" role="status">{normalizedQuery ? results.length ? t('docs.search.matches', { count: results.length }) : t('docs.search.no-matches') : ''}</p>
          <div className="docs-sidebar-note"><p>{text('docs.ui.found-a-gap-in-the-docs')}</p><a href={`${repository}/issues`}>{text('docs.ui.open-an-issue')}</a></div>
        </div>
      </aside>
      <main id="docs-main" className="docs-article">
        {page ? <article className="doc-section doc-page" aria-labelledby="doc-title">
          <div className="docs-breadcrumb">{text('docs.ui.docs', [<a href={docPath('overview')} />, <span />, <span className="docs-crumb" />, group?.title])}</div>
          <h1 id="doc-title">{page.title}</h1>
          <p className="doc-section-description">{page.description}</p>
          {pageContents[page.slug]}
          <nav className="docs-pager" aria-label={t('docs.ui.previous-and-next-page')}>
            {previous && <a className="docs-pager-prev" href={docPath(previous.slug)}>{text('docs.ui.previous', [<span />, <strong />, previous.title])}</a>}
            {next && <a className="docs-pager-next" href={docPath(next.slug)}>{text('docs.ui.next', [<span />, <strong />, next.title])}</a>}
          </nav>
        </article> : <Overview />}
        <div className="docs-end"><h2>{text('docs.ui.try-it-without-installing', [<em />])}</h2><p>{text('docs.ui.the-playground-runs-celesta-in-your-browser')}</p><a className="button button-primary" href={`${homePath}#playground`}>{text('docs.ui.open-the-playground', [<span aria-hidden="true" />])}</a></div>
        <footer className="docs-footer">{text('docs.ui.mit-apache-2-0-source-documentation', [<span />, <a href={`${repository}#readme`} />])}</footer>
      </main>
    </div>
  </div>;
}

function Overview() {
  return <section id="overview" className="docs-intro" aria-labelledby="docs.metadata.title">
    <div className="docs-breadcrumb">{text('docs.ui.celesta-docs-in-development', [<span />, <span className="docs-version" />])}</div>
    <h1 id="docs.metadata.title">{text('docs.ui.celesta-documentation', [<br />, <em />])}</h1>
    <p className="docs-lead">{text('docs.ui.install-the-app-write-a-composition-export', [<br />])}</p>
    <p>{text('docs.ui.celesta-is-a-code-first-video-tool')}</p>
    <div className="docs-paths grid sm:grid-cols-2 gap-4">
      <a href={docPath('installation')}><span>{text('docs.ui.start-here', [<span aria-hidden="true" />])}</span><strong>{text('docs.ui.install-celesta')}</strong><p>{text('docs.ui.download-the-app-and-open-your-first')}</p></a>
      <a href={docPath('react-compositions')}><span>{text('docs.ui.then', [<span aria-hidden="true" />])}</span><strong>{text('docs.ui.your-first-composition')}</strong><p>{text('docs.ui.a-five-second-react-title-card-start')}</p></a>
    </div>
    <div className="docs-workflow" aria-label={t('docs.ui.the-celesta-workflow')}>{text('docs.ui.01-write-in-your-editor-02-preview', [<span />, <b />, <i aria-hidden="true" />, <span />, <b />, <i aria-hidden="true" />, <span />, <b />])}</div>
    <p className="docs-development-note">{text('docs.ui.start-with-the-app-download-for-macos')}</p>
    <div className="docs-index">
      {docGroups.map(section => <div key={section.title}>
        <h2>{section.title}</h2>
        <ul>{section.pages.map(topic => <li key={topic.slug}><a href={docPath(topic.slug)}><strong>{topic.title}</strong><span>{topic.description}</span></a></li>)}</ul>
      </div>)}
    </div>
  </section>;
}
