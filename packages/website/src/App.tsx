import { useEffect, useRef, useState } from 'react';
import { Playground } from './Playground';
import { Brand } from './Brand';
import { DownloadAlternatives, DownloadButton } from './Download';
import { allDownloads, repository } from './links';

const documentation = '/docs/';

function formatTimecode(frame: number, fps = 30) {
  const seconds = Math.floor(frame / fps);
  return [Math.floor(seconds / 3600), Math.floor(seconds / 60) % 60, seconds % 60, frame % fps].map(part => String(part).padStart(2, '0')).join(':');
}

/** A running 30 fps timecode; it stays at zero for visitors who prefer reduced motion. */
function Timecode() {
  const output = useRef<HTMLSpanElement>(null);
  useEffect(() => {
    if (matchMedia('(prefers-reduced-motion: reduce)').matches) return;
    const start = performance.now();
    let request = 0;
    const tick = (now: number) => {
      if (output.current) output.current.textContent = formatTimecode(Math.floor((now - start) * 30 / 1000));
      request = requestAnimationFrame(tick);
    };
    request = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(request);
  }, []);
  return <span ref={output}>{formatTimecode(0)}</span>;
}

export function App() {
  const [menuOpen, setMenuOpen] = useState(false);
  return <>
    <a href="#main" className="skip-link">Skip to content</a>
    <header id="top" className="header">
      <div className="shell header-inner flex items-center justify-between">
        <Brand />
        <button className="menu-toggle" aria-expanded={menuOpen} aria-controls="navigation" onClick={() => setMenuOpen(!menuOpen)}>{menuOpen ? 'Close' : 'Menu'}</button>
        <nav id="navigation" aria-label="Main navigation" className={`nav flex items-center ${menuOpen ? 'is-open' : ''}`} onClick={() => setMenuOpen(false)}>
          <a href="#features">Why Celesta</a><a href="#playground">Playground</a><a href={documentation}>Docs</a><a href={repository}>GitHub <span aria-hidden="true">↗</span></a><a className="nav-download" href={allDownloads}>Download</a>
        </nav>
      </div>
    </header>
    <main id="main">
      <section className="hero shell" aria-labelledby="hero-title">
        <div className="hero-meta flex items-center justify-between gap-6">
          <a className="development-badge" href={`${repository}/commits`}>Open source · In active development <span aria-hidden="true">↗</span></a>
          <span className="hero-timecode" aria-hidden="true">TC <Timecode /></span>
        </div>
        <h1 id="hero-title">Motion,<br /><em>written down.</em></h1>
        <div className="hero-lower">
          <p className="hero-description">Celesta renders React components to video. Scrub to any frame in the browser or the desktop app, then export an MP4.</p>
          <div className="hero-cta">
            <div className="hero-actions flex flex-wrap gap-3"><DownloadButton /><a className="button button-secondary" href="#playground">Try it in the browser</a></div>
            <DownloadAlternatives className="hero-alternatives" />
          </div>
        </div>
        <dl className="hero-specs" aria-label="At a glance">
          <div><dt>01 Write</dt><dd>Plain React.<small>Components and hooks, written in your own editor.</small></dd></div>
          <div><dt>02 Preview</dt><dd>Frame-accurate.<small>Scrub in the browser, or in the desktop app with audio in sync.</small></dd></div>
          <div><dt>03 Export</dt><dd>H.264 and AAC.<small>MP4 files from the browser, the app, or the command line.</small></dd></div>
          <div><dt>04 License</dt><dd>MIT or Apache-2.0.<small>Free and open source. The engine is written in Rust.</small></dd></div>
        </dl>
      </section>
      <Playground />
      <section id="features" className="features shell" aria-labelledby="features-heading">
        <div className="section-top"><p className="eyebrow">Why Celesta</p><div><h2 id="features-heading">Every frame is<br /><em>a function call.</em></h2><p className="section-description"><code>useCurrentFrame()</code> gives you a number. Position, opacity, and text are ordinary React from there, so Celesta can render any frame on its own: for scrubbing, for previews, and for export.</p></div></div>
        <div className="feature-grid grid md:grid-cols-3">
          <article className="feature-card"><div className="feature-visual code-visual" aria-hidden="true"><span className="code-mini"><i>&lt;</i>YourNextIdea <i>/&gt;</i></span></div><span className="feature-number">01 — Compose</span><h3>Components and hooks.</h3><p>Build scenes from <code>Rect</code>, <code>Text</code>, <code>Image</code>, and <code>Group</code>. Animate with <code>interpolate()</code> and easing curves, and share pieces between projects like any other React code.</p><a href="/docs/react-compositions/">React compositions <span aria-hidden="true">→</span></a></article>
          <article className="feature-card"><div className="feature-visual timeline-visual" aria-hidden="true"><div className="mini-ruler"><span>00:00</span><span>00:02</span><span>00:04</span></div><div className="mini-track track-lilac">story.tsx</div><div className="mini-track track-peach">atmosphere</div><div className="mini-track track-sage">voice.wav</div><div className="mini-playhead" /></div><span className="feature-number">02 — Preview</span><h3>Scrub to any frame.</h3><p>The browser playground previews without sound. The desktop app plays audio in sync and reloads when you save.</p><a href="/docs/preview/">Previewing <span aria-hidden="true">→</span></a></article>
          <article className="feature-card"><div className="feature-visual export-visual" aria-hidden="true"><dl className="file-art"><div><dt>File</dt><dd>your-story.mp4</dd></div><div><dt>Video</dt><dd>H.264 · 1920×1080</dd></div><div><dt>Audio</dt><dd>AAC · 48 kHz</dd></div><div><dt>Frames</dt><dd>150 @ 30 fps</dd></div></dl></div><span className="feature-number">03 — Export</span><h3>A standard MP4.</h3><p>H.264 video with AAC audio, ready for any player or upload. Export from the browser, the desktop app, or the command line.</p><a href="/docs/export/">Exporting <span aria-hidden="true">→</span></a></article>
        </div>
      </section>
      <section className="possibilities shell" aria-labelledby="possibilities-heading">
        <div className="section-top"><p className="eyebrow">Good for</p><div><h2 id="possibilities-heading">Video you would rather<br /><em>describe than drag.</em></h2></div></div>
        <ul className="idea-tags">{['Title sequences', 'Dialogue with lip sync', 'Data-driven video', 'Generative loops'].map((idea, i) => <li key={idea}><span>{String(i + 1).padStart(2, '0')}</span>{idea}</li>)}</ul>
      </section>
      <section id="start" className="start-section" aria-labelledby="start-heading">
        <div className="shell start-card grid md:grid-cols-2">
          <div><p className="eyebrow">Get started</p><h2 id="start-heading">Start with<br /><em>one file.</em></h2><p>The desktop app ships with Node.js and FFmpeg, so there is nothing else to install. Celesta is early and changing quickly; bug reports and ideas are welcome on GitHub.</p><DownloadButton /><DownloadAlternatives className="start-note" /></div>
          <div className="getting-started"><span className="eyebrow">Setup</span><ol><li><span>01</span><div><h3>Install the app.</h3><p>Packages are available for macOS and Windows.</p></div></li><li><span>02</span><div><h3>Write a composition.</h3><p>A single <code>.tsx</code> file, in any folder.</p></div></li><li><span>03</span><div><h3>Open, preview, export.</h3><p>Open the file in Celesta. It reloads on save, and exports to MP4 when you are ready.</p></div></li></ol><a href="/docs/examples/">Browse the examples <span aria-hidden="true">→</span></a></div>
        </div>
      </section>
    </main>
    <footer className="footer shell">
      <div className="footer-top"><div><Brand /><p>Code-first video. Open source.</p></div><nav aria-label="Footer"><a href={documentation}>Documentation</a><a href={repository}>GitHub</a><a href={`${repository}/releases`}>Releases</a><a href={`${repository}/issues`}>Issues</a></nav></div>
      <div className="footer-bottom flex flex-wrap justify-between gap-3"><span>Built with Rust and TypeScript.</span><span>MIT / Apache-2.0</span></div>
    </footer>
  </>;
}
