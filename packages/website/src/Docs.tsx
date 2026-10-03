import { useEffect, useState } from 'react';
import { Brand } from './Brand';
import { contents, repository } from './docs-content';
import { docGroups, docPages, docPath } from './docs-nav';
import { packageContents } from './docs-packages';

const pageContents = { ...contents, ...packageContents };

const overviewTopic = { slug: 'overview', title: 'Overview', description: 'What Celesta is and how the workflow fits together.', keywords: 'introduction start overview welcome' };

/** The page for the current address: `/docs/` is the overview, `/docs/<slug>/` a chapter. */
function currentSlug(): string {
  const slug = window.location.pathname.replace(/^\/docs\/?/, '').replace(/\/+$/, '');
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
    if (page) document.title = `${page.title} — Celesta documentation`;
    // The page's content is mounted by React, after the HTML is loaded.
    const frame = requestAnimationFrame(() => {
      if (window.location.hash) document.getElementById(window.location.hash.slice(1))?.scrollIntoView({ behavior: 'instant' });
    });
    return () => cancelAnimationFrame(frame);
  }, [page]);

  const topicLink = (topic: { slug: string; title: string }) => <a key={topic.slug} href={docPath(topic.slug)} aria-current={slug === topic.slug ? 'page' : undefined}>{topic.title}</a>;

  return <div className="docs-page">
    <a className="skip-link" href="#docs-main">Skip to documentation</a>
    <header className="header docs-header">
      <div className="shell header-inner flex items-center justify-between">
        <div className="flex items-center gap-5"><Brand /><a className="docs-header-label" href="/docs/">Documentation</a></div>
        <nav className="docs-top-nav flex items-center gap-6" aria-label="Main navigation"><a href="/">Website</a><a href={repository}>GitHub <span aria-hidden="true">↗</span></a></nav>
      </div>
    </header>
    <div className="docs-layout shell">
      <aside className="docs-sidebar">
        <button className="docs-contents-toggle" aria-expanded={contentsOpen} aria-controls="docs-contents" onClick={() => setContentsOpen(!contentsOpen)}>{page ? page.title : 'Documentation'} <span aria-hidden="true">{contentsOpen ? '−' : '+'}</span></button>
        <div id="docs-contents" className={`docs-contents ${contentsOpen ? 'is-open' : ''}`}>
          <label className="docs-search-label" htmlFor="topic-search">Find a topic</label>
          <div className="docs-search"><span aria-hidden="true">⌕</span><input id="topic-search" type="search" placeholder="Setup, math, export…" value={query} onChange={event => setQuery(event.target.value)} />{query && <button onClick={() => setQuery('')} aria-label="Clear topic search">×</button>}</div>
          <nav aria-label="Documentation topics">
            {normalizedQuery
              ? results.map(topicLink)
              : <>
                <a href="/docs/" aria-current={slug === 'overview' ? 'page' : undefined}>Overview</a>
                {docGroups.map(section => <div key={section.title} className="docs-nav-group">
                  <p className="docs-nav-label">{section.title}</p>
                  {section.pages.map(topicLink)}
                </div>)}
              </>}
          </nav>
          <p className="docs-search-status" role="status">{normalizedQuery ? results.length ? `${results.length} matching ${results.length === 1 ? 'topic' : 'topics'}` : 'No matching topics. Try “React”, “FFmpeg”, or “export”.' : ''}</p>
          <div className="docs-sidebar-note"><p>Found a gap in the docs?</p><a href={`${repository}/issues`}>Open an issue ↗</a></div>
        </div>
      </aside>
      <main id="docs-main" className="docs-article">
        {page ? <article className="doc-section doc-page" aria-labelledby="doc-title">
          <div className="docs-breadcrumb"><a href="/docs/">Docs</a> <span>/</span> <span className="docs-crumb">{group?.title}</span></div>
          <h1 id="doc-title">{page.title}</h1>
          <p className="doc-section-description">{page.description}</p>
          {pageContents[page.slug]}
          <nav className="docs-pager" aria-label="Previous and next page">
            {previous && <a className="docs-pager-prev" href={docPath(previous.slug)}><span>Previous</span><strong>{previous.title}</strong></a>}
            {next && <a className="docs-pager-next" href={docPath(next.slug)}><span>Next</span><strong>{next.title}</strong></a>}
          </nav>
        </article> : <Overview />}
        <div className="docs-end"><h2>Try it <em>without installing.</em></h2><p>The playground runs Celesta in your browser, export included.</p><a className="button button-primary" href="/#playground">Open the playground <span aria-hidden="true">→</span></a></div>
        <footer className="docs-footer"><span>MIT / Apache-2.0</span><a href={`${repository}#readme`}>Source documentation ↗</a></footer>
      </main>
    </div>
  </div>;
}

function Overview() {
  return <section id="overview" className="docs-intro" aria-labelledby="docs-title">
    <div className="docs-breadcrumb">Celesta <span>/</span> Docs <span className="docs-version">In development</span></div>
    <h1 id="docs-title">Celesta<br /><em>documentation.</em></h1>
    <p className="docs-lead">Install the app, write a composition, export an MP4.<br />Read in order, or jump to a topic.</p>
    <p>Celesta is a code-first video tool. Compose animated scenes with React, preview them in the desktop app, and export an MP4.</p>
    <div className="docs-paths grid sm:grid-cols-2 gap-4">
      <a href={docPath('installation')}><span>Start here <span aria-hidden="true">→</span></span><strong>Install Celesta.</strong><p>Download the app and open your first scene.</p></a>
      <a href={docPath('react-compositions')}><span>Then <span aria-hidden="true">→</span></span><strong>Your first composition.</strong><p>A five-second React title card, start to finish.</p></a>
    </div>
    <div className="docs-workflow" aria-label="The Celesta workflow"><span><b>01</b> Write in your editor</span><i aria-hidden="true">→</i><span><b>02</b> Preview in Celesta</span><i aria-hidden="true">→</i><span><b>03</b> Export an MP4</span></div>
    <p className="docs-development-note">Start with the app download for macOS or Windows. Source-build instructions are included for developers and Linux users. Celesta is open source under MIT or Apache-2.0.</p>
    <div className="docs-index">
      {docGroups.map(section => <div key={section.title}>
        <h2>{section.title}</h2>
        <ul>{section.pages.map(topic => <li key={topic.slug}><a href={docPath(topic.slug)}><strong>{topic.title}</strong><span>{topic.description}</span></a></li>)}</ul>
      </div>)}
    </div>
  </section>;
}
