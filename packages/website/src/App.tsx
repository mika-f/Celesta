import { useState } from 'react';
import { Playground } from './Playground';
import { Brand } from './Brand';
import { DownloadAlternatives, DownloadButton } from './Download';
import { allDownloads, repository } from './links';

const documentation = '/docs/';

export function App() {
  const [menuOpen, setMenuOpen] = useState(false);
  return <>
    <a href="#main" className="skip-link">Skip to content</a>
    <header id="top" className="shell header flex items-center justify-between">
      <Brand />
      <button className="menu-toggle" aria-expanded={menuOpen} aria-controls="navigation" onClick={() => setMenuOpen(!menuOpen)}>{menuOpen ? 'Close' : 'Menu'} <span aria-hidden="true">{menuOpen ? '×' : '+'}</span></button>
      <nav id="navigation" aria-label="Main navigation" className={`nav flex items-center ${menuOpen ? 'is-open' : ''}`} onClick={() => setMenuOpen(false)}>
        <a href="#features">Why Celesta</a><a href="#playground">Playground</a><a href={documentation}>Docs <span aria-hidden="true">↗</span></a><a href={repository}>GitHub <span aria-hidden="true">↗</span></a><a className="nav-download" href={allDownloads}>Download <span aria-hidden="true">↓</span></a>
      </nav>
    </header>
    <main id="main">
      <section className="hero shell text-center" aria-labelledby="hero-title">
        <a className="development-badge inline-flex items-center gap-2" href={`${repository}/commits`}><span className="status-dot" /> Open source. Open possibilities. <span aria-hidden="true">↗</span></a>
        <div className="hero-title-wrap"><span className="hero-spark spark-left" aria-hidden="true">✧</span><h1 id="hero-title">A little code.<br /><em>A world in motion.</em></h1><span className="hero-spark spark-right" aria-hidden="true">✦</span></div>
        <p className="hero-description">Turn your ideas into videos with React.<br className="desktop-break" /> Compose in code, preview every frame, and let your story move.</p>
        <div className="hero-actions flex flex-wrap justify-center gap-3"><DownloadButton /><a className="button button-secondary" href="#playground"><span aria-hidden="true">▷</span> Take it for a spin</a></div>
        <DownloadAlternatives className="hero-alternatives" />
        <p className="hero-footnote">Free & open source <span>·</span> Built with Rust <span>·</span> Made for your imagination</p>
      </section>
      <Playground />
      <section className="principles shell grid grid-cols-1 md:grid-cols-3" aria-label="At a glance">
        <div><span aria-hidden="true">⌘</span><p>Your editor. Your flow.<small>Write React or a simple JSON timeline.</small></p></div>
        <div><span aria-hidden="true">◫</span><p>Every frame, considered.<small>Preview and scrub in a native app.</small></p></div>
        <div><span aria-hidden="true">↗</span><p>From idea to MP4.<small>Export with H.264 video and AAC audio.</small></p></div>
      </section>
      <section id="features" className="features shell" aria-labelledby="features-heading">
        <div className="section-top flex items-end justify-between gap-8"><div><p className="eyebrow">LESS FRICTION. MORE CREATION.</p><h2 id="features-heading">Familiar tools.<br /><em>Unfamiliar possibilities.</em></h2></div><p className="section-description">For the developers who think in pictures.<br />And the storytellers who like to tinker.</p></div>
        <div className="feature-grid grid md:grid-cols-3 gap-5">
          <article className="feature-card"><div className="feature-visual code-visual" aria-hidden="true"><span className="code-mini">&lt;YourNextIdea /&gt;</span><span className="visual-tag">React, meet the timeline.</span></div><span className="feature-number">01 / COMPOSE</span><h3>If you can code it,<br />you can move it.</h3><p>Build scenes with components, hooks, and animation helpers. Reuse what works. Change a prop. Make something entirely your own.</p><a href="/docs/#react-compositions">Explore React compositions <span aria-hidden="true">↗</span></a></article>
          <article className="feature-card"><div className="feature-visual timeline-visual" aria-hidden="true"><div className="mini-ruler"><span>00:00</span><span>00:02</span><span>00:04</span></div><div className="mini-track track-lilac">Your story.tsx</div><div className="mini-track track-peach">A little atmosphere</div><div className="mini-track track-sage">♫ A voice to remember</div><div className="mini-playhead" /></div><span className="feature-number">02 / PREVIEW</span><h3>Find the feeling.<br />Frame by frame.</h3><p>Play, pause, and get the timing just right. See your changes in a native preview, with synchronized audio and a timeline you can scrub.</p><a href="/docs/#preview">Meet the preview workflow <span aria-hidden="true">↗</span></a></article>
          <article className="feature-card"><div className="feature-visual export-visual" aria-hidden="true"><div className="file-art"><span>✦</span><b>your-story</b><small>.mp4</small></div><span className="export-check">✓</span><span className="export-orbit" /></div><span className="feature-number">03 / SHARE</span><h3>A real video.<br />Ready for the world.</h3><p>Export your composition as an MP4, from the app or the command line. Render the whole story, or just the moment you want to share.</p><a href="/docs/#export">See how to export <span aria-hidden="true">↗</span></a></article>
        </div>
      </section>
      <section className="possibilities shell flex flex-col md:flex-row items-start md:items-center justify-between gap-6"><div><p className="eyebrow">WHAT WILL YOU MAKE?</p><h2>Small experiments. <em>Big main-character energy.</em></h2></div><div className="idea-tags flex flex-wrap gap-2"><span>Motion graphics</span><span>Product stories</span><span>Character dialogue</span><span>Creative coding</span></div></section>
      <section id="start" className="start-section shell" aria-labelledby="start-heading">
        <div className="start-card grid md:grid-cols-2 gap-10">
          <div><span className="start-spark" aria-hidden="true">✦</span><p className="eyebrow">YOUR NEXT IDEA STARTS HERE</p><h2 id="start-heading">Make something<br /><em>only you could make.</em></h2><p>Celesta is young, open source, and growing.<br />Come build your first scene. Help shape what comes next.</p><DownloadButton /><DownloadAlternatives className="start-note" /></div>
          <div className="getting-started"><span className="eyebrow">A LITTLE SETUP, THEN YOU’RE OFF.</span><ol><li><span>01</span><div><h3>Make yourself at home.</h3><p>Download Celesta for macOS or Windows and install the app.</p></div></li><li><span>02</span><div><h3>Start with a little idea.</h3><p>Save a React composition or JSON timeline in your own project folder.</p></div></li><li><span>03</span><div><h3>Give your idea a first frame.</h3><p>Open your file in Celesta, preview it, and export an MP4. The runtime is included.</p></div></li></ol><a href="/docs/#examples">Find a little inspiration in the examples <span aria-hidden="true">↗</span></a></div>
        </div>
      </section>
    </main>
    <footer className="shell footer"><div className="flex flex-wrap items-center justify-between gap-6"><Brand /><p>A little tool for the things you haven’t made yet.</p><div className="flex gap-6"><a href={repository}>GitHub ↗</a><a href={documentation}>Documentation ↗</a></div></div><div className="footer-bottom flex flex-wrap justify-between gap-3"><span>Made with care. Shared with everyone.</span><span>MIT / Apache-2.0 <span aria-hidden="true">✦</span></span></div></footer>
  </>;
}
