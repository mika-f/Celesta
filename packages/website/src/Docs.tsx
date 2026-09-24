import { useEffect, useState } from 'react';
import { Brand } from './Brand';
import { repository, sections } from './docs-content';

const topics = [
  { id: 'overview', title: 'Welcome to Celesta', description: 'An overview of the creative workflow.', keywords: 'introduction start overview' },
  ...sections,
];

export function Docs() {
  const [query, setQuery] = useState('');
  const [contentsOpen, setContentsOpen] = useState(false);
  const [activeId, setActiveId] = useState('overview');
  const normalizedQuery = query.trim().toLowerCase();
  const results = topics.filter(topic => `${topic.title} ${topic.description} ${topic.keywords}`.toLowerCase().includes(normalizedQuery));

  useEffect(() => {
    function syncHash() {
      const id = window.location.hash.slice(1) || 'overview';
      if (topics.some(topic => topic.id === id)) setActiveId(id);
    }
    syncHash();
    // The document's anchors are mounted by React, after the HTML is loaded.
    const frame = requestAnimationFrame(() => {
      if (window.location.hash) document.getElementById(window.location.hash.slice(1))?.scrollIntoView({ behavior: 'instant' });
    });
    window.addEventListener('hashchange', syncHash);
    return () => {
      cancelAnimationFrame(frame);
      window.removeEventListener('hashchange', syncHash);
    };
  }, []);

  function selectTopic(id: string) {
    setActiveId(id);
    setContentsOpen(false);
    setQuery('');
  }

  return <div className="docs-page">
    <a className="skip-link" href="#docs-main">Skip to documentation</a>
    <header className="shell header docs-header flex items-center justify-between">
      <div className="flex items-center gap-5"><Brand /><span className="docs-header-label">Documentation</span></div>
      <nav className="docs-top-nav flex items-center gap-6" aria-label="Main navigation"><a href="/">Website</a><a href={repository}>GitHub <span aria-hidden="true">↗</span></a></nav>
    </header>
    <div className="docs-layout shell">
      <aside className="docs-sidebar">
        <button className="docs-contents-toggle" aria-expanded={contentsOpen} aria-controls="docs-contents" onClick={() => setContentsOpen(!contentsOpen)}>On this page <span aria-hidden="true">{contentsOpen ? '−' : '+'}</span></button>
        <div id="docs-contents" className={`docs-contents ${contentsOpen ? 'is-open' : ''}`}>
          <label className="docs-search-label" htmlFor="topic-search">Find a topic</label>
          <div className="docs-search"><span aria-hidden="true">⌕</span><input id="topic-search" type="search" placeholder="Setup, React, export…" value={query} onChange={event => setQuery(event.target.value)} />{query && <button onClick={() => setQuery('')} aria-label="Clear topic search">×</button>}</div>
          <p className="docs-nav-label">THE FIELD GUIDE</p>
          <nav aria-label="Documentation topics">
            {results.map((topic) => <a key={topic.id} href={`#${topic.id}`} aria-current={activeId === topic.id ? 'location' : undefined} onClick={() => selectTopic(topic.id)}>{topic.title}</a>)}
          </nav>
          <p className="docs-search-status" role="status">{normalizedQuery ? results.length ? `${results.length} matching ${results.length === 1 ? 'topic' : 'topics'}` : 'No matching topics. Try “React”, “FFmpeg”, or “export”.' : ''}</p>
          <div className="docs-sidebar-note"><span aria-hidden="true">✦</span><p>A little guidance for<br />your next big idea.</p><a href={`${repository}/issues`}>Something missing? ↗</a></div>
        </div>
      </aside>
      <main id="docs-main" className="docs-article">
        <section id="overview" className="docs-intro" aria-labelledby="docs-title">
          <div className="docs-breadcrumb">CELESTA <span>/</span> DOCUMENTATION <span className="docs-version">IN DEVELOPMENT</span></div>
          <h1 id="docs-title">A little guidance.<br /><em>A lot to create.</em></h1>
          <p className="docs-lead">From your first frame to your finished video.<br />Let’s make something move.</p>
          <p>Celesta is a code-first video tool. Describe a timeline with JSON or compose animated scenes with React, preview them in a native app, and export an MP4.</p>
          <div className="docs-paths grid sm:grid-cols-2 gap-4">
            <a href="#installation"><span>START HERE <span aria-hidden="true">↗</span></span><strong>Set up your creative space.</strong><p>Download the app and open your first scene.</p></a>
            <a href="#react-compositions"><span>MAKE YOUR FIRST SCENE <span aria-hidden="true">↗</span></span><strong>A title. A few lines. A little motion.</strong><p>Build a five-second React composition.</p></a>
          </div>
          <div className="docs-workflow" aria-label="The Celesta workflow"><span><b>01</b> Write in your editor</span><i aria-hidden="true">→</i><span><b>02</b> Preview in Celesta</span><i aria-hidden="true">→</i><span><b>03</b> Export an MP4</span></div>
          <p className="docs-development-note">Start with the app download for macOS or Windows. Source-build instructions are included for developers and Linux users. Celesta is open source under MIT or Apache-2.0.</p>
        </section>
        {sections.map((section, index) => <section id={section.id} key={section.id} className="doc-section" aria-labelledby={`${section.id}-heading`}>
          <p className="eyebrow">{String(index + 1).padStart(2, '0')} / THE FIELD GUIDE</p>
          <h2 id={`${section.id}-heading`}><a href={`#${section.id}`}>{section.title}<span className="doc-heading-anchor" aria-hidden="true">#</span></a></h2>
          <p className="doc-section-description">{section.description}</p>
          {section.content}
        </section>)}
        <div className="docs-end"><span aria-hidden="true">✦</span><h2>Your next frame is waiting.</h2><p>Try a little idea in the playground, then bring it to life in Celesta.</p><a className="button button-primary" href="/#playground">Back to the playground <span aria-hidden="true">↗</span></a></div>
        <footer className="docs-footer"><span>Made with care. Shared with everyone.</span><a href={`${repository}#readme`}>Source documentation ↗</a></footer>
      </main>
    </div>
  </div>;
}
