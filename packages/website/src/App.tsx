import { LanguageSwitcher } from './LanguageSwitcher';
import { docPath } from './docs-nav';
import { t, text } from './i18n';
import { useEffect, useRef, useState } from 'react';
import { Playground } from './Playground';
import { Brand } from './Brand';
import { DownloadAlternatives, DownloadButton } from './Download';
import { allDownloads, repository } from './links';

const documentation = docPath('overview');

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
    <a href="#main" className="skip-link">{text('home.skip-to-content')}</a>
    <header id="top" className="header">
      <div className="shell header-inner flex items-center justify-between">
        <Brand />
        <button className="menu-toggle" aria-expanded={menuOpen} aria-controls="navigation" onClick={() => setMenuOpen(!menuOpen)}>{menuOpen ? t('close') : t('menu')}</button>
        <nav id="navigation" aria-label={t('home.main-navigation')} className={`nav flex items-center ${menuOpen ? 'is-open' : ''}`} onClick={() => setMenuOpen(false)}>{text('home.why-celesta-playground-docs-github-download', [<a href="#features" />, <a href="#playground" />, <a href={documentation} />, <a href={repository} />, <span aria-hidden="true" />, <a className="nav-download" href={allDownloads} />])}<LanguageSwitcher /></nav>
      </div>
    </header>
    <main id="main">
      <section className="hero shell" aria-labelledby="hero-title">
        <div className="hero-meta flex items-center justify-between gap-6">{text('home.open-source-in-active-development-tc', [<a className="development-badge" href={`${repository}/commits`} />, <span aria-hidden="true" />, <span className="hero-timecode" aria-hidden="true" />, <Timecode />])}</div>
        <h1 id="hero-title">{text('home.motion-written-down', [<br />, <em />])}</h1>
        <div className="hero-lower">
          <p className="hero-description">{text('home.celesta-renders-react-components-to-video-scrub')}</p>
          <div className="hero-cta">
            <div className="hero-actions flex flex-wrap gap-3">{text('home.try-it-in-the-browser', [<DownloadButton />, <a className="button button-secondary" href="#playground" />])}</div>
            <DownloadAlternatives className="hero-alternatives" />
          </div>
        </div>
        <dl className="hero-specs" aria-label={t('home.at-a-glance')}>
          <div><dt>{text('home.01-write')}</dt><dd>{text('home.plain-react-components-and-hooks-written-in', [<small />])}</dd></div>
          <div><dt>{text('home.02-preview')}</dt><dd>{text('home.frame-accurate-scrub-in-the-browser-or', [<small />])}</dd></div>
          <div><dt>{text('home.03-export')}</dt><dd>{text('home.h-264-and-aac-mp4-files-from', [<small />])}</dd></div>
          <div><dt>{text('home.04-license')}</dt><dd>{text('home.mit-or-apache-2-0-free-and', [<small />])}</dd></div>
        </dl>
      </section>
      <Playground />
      <section id="features" className="features shell" aria-labelledby="features-heading">
        <div className="section-top"><p className="eyebrow">{text('home.why-celesta')}</p><div><h2 id="features-heading">{text('home.every-frame-is-a-function-call', [<br />, <em />])}</h2><p className="section-description">{text('home.gives-you-a-number-position-opacity-and', [<code>useCurrentFrame()</code>])}</p></div></div>
        <div className="feature-grid grid md:grid-cols-3">
          <article className="feature-card"><div className="feature-visual code-visual" aria-hidden="true">{text('home.yournextidea', [<span className="code-mini" />, <i />, <i />])}</div><span className="feature-number">{text('home.01-compose')}</span><h3>{text('home.components-and-hooks')}</h3><p>{text('home.build-scenes-from-and-animate-with-and', [<code>Rect</code>, <code>Text</code>, <code>Image</code>, <code>Group</code>, <code>interpolate()</code>])}</p><a href={docPath('react-compositions')}>{text('home.react-compositions', [<span aria-hidden="true" />])}</a></article>
          <article className="feature-card"><div className="feature-visual timeline-visual" aria-hidden="true"><div className="mini-ruler"><span>00:00</span><span>00:02</span><span>00:04</span></div><div className="mini-track track-lilac">{text('home.story-tsx')}</div><div className="mini-track track-peach">{text('home.atmosphere')}</div><div className="mini-track track-sage">{text('home.voice-wav')}</div><div className="mini-playhead" /></div><span className="feature-number">{text('home.preview-step')}</span><h3>{text('home.scrub-to-any-frame')}</h3><p>{text('home.the-browser-playground-previews-without-sound-the')}</p><a href={docPath('preview')}>{text('home.previewing', [<span aria-hidden="true" />])}</a></article>
          <article className="feature-card"><div className="feature-visual export-visual" aria-hidden="true"><dl className="file-art"><div><dt>{text('home.file')}</dt><dd>{text('home.your-story-mp4')}</dd></div><div><dt>{text('home.video')}</dt><dd>{text('home.h-264-1920-1080')}</dd></div><div><dt>{text('home.audio')}</dt><dd>{text('home.aac-48-khz')}</dd></div><div><dt>{text('home.frames')}</dt><dd>{text('home.150-30-fps')}</dd></div></dl></div><span className="feature-number">{text('home.export-step')}</span><h3>{text('home.a-standard-mp4')}</h3><p>{text('home.h-264-video-with-aac-audio-ready')}</p><a href={docPath('export')}>{text('home.exporting', [<span aria-hidden="true" />])}</a></article>
        </div>
      </section>
      <section className="possibilities shell" aria-labelledby="possibilities-heading">
        <div className="section-top"><p className="eyebrow">{text('home.good-for')}</p><div><h2 id="possibilities-heading">{text('home.video-you-would-rather-describe-than-drag', [<br />, <em />])}</h2></div></div>
        <ul className="idea-tags">{[t('idea-title'), t('idea-dialogue'), t('idea-data'), t('idea-loops')].map((idea, i) => <li key={idea}><span>{String(i + 1).padStart(2, '0')}</span>{idea}</li>)}</ul>
      </section>
      <section id="start" className="start-section" aria-labelledby="start-heading">
        <div className="shell start-card grid md:grid-cols-2">
          <div><p className="eyebrow">{text('home.get-started')}</p><h2 id="start-heading">{text('home.start-with-one-file', [<br />, <em />])}</h2><p>{text('home.the-desktop-app-ships-with-node-js')}</p><DownloadButton /><DownloadAlternatives className="start-note" /></div>
          <div className="getting-started"><span className="eyebrow">{text('home.setup')}</span><ol><li><span>01</span><div><h3>{text('home.install-the-app')}</h3><p>{text('home.packages-are-available-for-macos-and-windows')}</p></div></li><li><span>02</span><div><h3>{text('home.write-a-composition')}</h3><p>{text('home.a-single-file-in-any-folder', [<code>.tsx</code>])}</p></div></li><li><span>03</span><div><h3>{text('home.open-preview-export')}</h3><p>{text('home.open-the-file-in-celesta-it-reloads')}</p></div></li></ol><a href={docPath('examples')}>{text('home.browse-the-examples', [<span aria-hidden="true" />])}</a></div>
        </div>
      </section>
    </main>
    <footer className="footer shell">
      <div className="footer-top"><div><Brand /><p>{text('home.code-first-video-open-source')}</p></div><nav aria-label={t('home.footer')}>{text('home.documentation-github-releases-issues', [<a href={documentation} />, <a href={repository} />, <a href={`${repository}/releases`} />, <a href={`${repository}/issues`} />])}</nav></div>
      <div className="footer-bottom flex flex-wrap justify-between gap-3">{text('home.built-with-rust-and-typescript-mit-apache', [<span />, <span />])}</div>
    </footer>
  </>;
}
