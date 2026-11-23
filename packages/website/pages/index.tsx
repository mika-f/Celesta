import { RouteLink } from '@natsuneko-laboratory/makit-runtime';
import type { StaticPageComponentProps } from '@natsuneko-laboratory/makit-runtime';
import { ArrowRight, ArrowUpRight } from 'lucide-react';
import type { ReactNode } from 'react';
import { copy, isLocale } from '../site/copy/index.ts';
import { downloadLinks, type Platform } from '../site/downloads.ts';
import { repository } from '../site/links.ts';
import { DownloadAlternatives, DownloadButton } from './_components/DownloadButton.tsx';
import { PlaygroundMount } from './_components/PlaygroundMount.tsx';
import { Timecode } from './_components/Timecode.tsx';
import './_components/landing.css';

const platforms: Platform[] = ['macOS', 'Windows'];
const br = () => <br />;
const em = (children?: ReactNode) => <em>{children}</em>;
const hidden = (children?: ReactNode) => <span aria-hidden="true">{children}</span>;
const external = () => <ArrowUpRight size={14} aria-hidden="true" />;
const forward = () => <ArrowRight size={16} strokeWidth={1.75} aria-hidden="true" />;

/** The landing page: hero, playground, features, ideas, and getting started. */
export default function Landing({ locale: urlLocale }: StaticPageComponentProps) {
  const locale = isLocale(urlLocale) ? urlLocale : 'en';
  const { home, download, common } = copy(locale);
  const docs = (slug: string) => `/${locale}/docs/${slug}/`;
  const docsLink = (slug: string, label: ReactNode) => <RouteLink className="lp-more" href={docs(slug)}>{label}</RouteLink>;

  // Resolved while prerendering; the button picks the visitor's platform in the browser.
  const links = downloadLinks();
  const button = <DownloadButton links={links} labels={{ any: download.label, macOS: download.downloadPlatform({ platform: 'macOS' }), Windows: download.downloadPlatform({ platform: 'Windows' }) }} />;
  const alternatives = (current: Platform | null) => download.installGuideAllReleases(
    () => (current ? download.alsoFor : download.for),
    () => platforms.filter(platform => platform !== current).map((platform, i) => <span key={platform}>{i > 0 && download.and}<a href={links[platform]}>{platform}</a></span>),
    hidden,
    children => <RouteLink href={docs('installation')}>{children}</RouteLink>,
    hidden,
    children => <a href={links.all}>{children}</a>,
    external,
  );
  const variants = { any: alternatives(null), macOS: alternatives('macOS'), Windows: alternatives('Windows') };

  return <div className="lp">
    <section className="lp-hero" aria-labelledby="hero-title">
      <div className="nr-shell">
        <p className="lp-hero-meta nr-eyebrow">{home.openSourceInActiveDevelopmentTc(
          children => <a className="lp-badge" href={`${repository}/commits`}>{children}</a>,
          external,
          () => null,
          () => null,
        )}</p>
        <h1 id="hero-title">{home.motionWrittenDown(br, em)}</h1>
        <div className="lp-hero-lower">
          <p className="lp-hero-description">{home.celestaRendersReactComponentsToVideoScrub}</p>
          <div className="lp-hero-cta">
            <div className="lp-actions">{home.tryItInTheBrowser(() => button, children => <a className="nr-button-secondary" href="#playground">{children}</a>)}</div>
            <DownloadAlternatives variants={variants} />
          </div>
        </div>
        {/* The rail: three clips on a ten-second track, a sweeping playhead, and the running timecode. */}
        <div className="lp-timeline" aria-hidden="true">
          <div className="nr-rail lp-rail"><i className="nr-rail-clip">{home.storyTsx}</i><i className="nr-rail-clip">{home.atmosphere}</i><i className="nr-rail-clip">{home.voiceWav}</i><span className="nr-rail-head" /></div>
          <div className="nr-rail-tc"><span>00:00</span><b>{home.openSourceInActiveDevelopmentTc(() => null, () => null, children => children, () => <Timecode />)}</b><span>00:10</span></div>
        </div>
      </div>
      <dl className="lp-specs nr-shell" aria-label={home.atAGlance}>
        <div><dt>{home['01Write']}</dt><dd>{home.reactAndTypescriptWrittenIn(children => <small>{children}</small>)}</dd></div>
        <div><dt>{home['02Preview']}</dt><dd>{home.scrubAndAdjustInTheBrowserOr(children => <small>{children}</small>)}</dd></div>
        <div><dt>{home['03Export']}</dt><dd>{home.videoGifAndStillsFrom(children => <small>{children}</small>)}</dd></div>
        <div><dt>{home['04Extend']}</dt><dd>{home.pluginsAndMcpOpenSource(children => <small>{children}</small>)}</dd></div>
      </dl>
    </section>

    <section id="playground" className="lp-playground nr-shell" aria-label={common.navigation.playground}>
      <PlaygroundMount options={{ kind: 'home', locale }} fallback={<p className="lp-fallback">{home.metadata.noscript(children => <a href={`${repository}#readme`}>{children}</a>)}</p>} />
    </section>

    <section id="features" className="lp-section nr-shell" aria-labelledby="features-heading">
      <div className="lp-section-top">
        <p className="lp-eyebrow">{home.whyCelesta}</p>
        <div>
          <h2 id="features-heading">{home.everyFrameIsAFunctionCall(br, em)}</h2>
          <p className="lp-section-description">{home.givesYouANumberPositionOpacityAnd(() => <code>useCurrentFrame()</code>)}</p>
        </div>
      </div>
      <div className="lp-features">
        <article className="lp-feature">
          <div className="lp-feature-visual lp-code-visual" aria-hidden="true">{home.yournextidea(children => <span className="nr-mono">{children}</span>, children => <i>{children}</i>, children => <i>{children}</i>)}</div>
          <span className="lp-feature-number">{home['01Compose']}</span>
          <h3>{home.componentsAndHooks}</h3>
          <p>{home.buildScenesFromAndAnimateWithAnd(() => <code>Rect</code>, () => <code>Text</code>, () => <code>Image</code>, () => <code>Group</code>, () => <code>interpolate()</code>)}</p>
          {docsLink('react-compositions', home.reactCompositions(forward))}
        </article>
        <article className="lp-feature">
          <div className="lp-feature-visual lp-track-visual nr-mono" aria-hidden="true">
            <div className="lp-mini-ruler"><span>00:00</span><span>00:02</span><span>00:04</span></div>
            <div className="lp-mini-track">{home.storyTsx}</div>
            <div className="lp-mini-track">{home.atmosphere}</div>
            <div className="lp-mini-track">{home.voiceWav}</div>
            <div className="lp-mini-playhead" />
          </div>
          <span className="lp-feature-number">{home.previewStep}</span>
          <h3>{home.scrubToAnyFrame}</h3>
          <p>{home.theBrowserPlaygroundPreviewsWithoutSoundThe}</p>
          {docsLink('preview', home.previewing(forward))}
        </article>
        <article className="lp-feature">
          <div className="lp-feature-visual lp-export-visual" aria-hidden="true">
            <dl className="lp-file-art nr-mono">
              <div><dt>{home.file}</dt><dd>{home.yourStoryMp4}</dd></div>
              <div><dt>{home.video}</dt><dd>{home.h26419201080}</dd></div>
              <div><dt>{home.audio}</dt><dd>{home.aac48Khz}</dd></div>
              <div><dt>{home.frames}</dt><dd>{home['15030Fps']}</dd></div>
            </dl>
          </div>
          <span className="lp-feature-number">{home.exportStep}</span>
          <h3>{home.readyToPublish}</h3>
          <p>{home.exportFormatsReadyForAnyPlayer}</p>
          {docsLink('export', home.exporting(forward))}
        </article>
      </div>
    </section>

    <section className="lp-section nr-shell" aria-labelledby="ideas-heading">
      <div className="lp-section-top">
        <p className="lp-eyebrow">{home.goodFor}</p>
        <div><h2 id="ideas-heading">{home.videoYouWouldRatherDescribeThanDrag(br, em)}</h2></div>
      </div>
      <ul className="lp-ideas">{[home.ideas.title, home.ideas.dialogue, home.ideas.data, home.ideas.loops].map((idea, i) =>
        <li key={idea}><span>{String(i + 1).padStart(2, '0')}</span>{idea}</li>)}</ul>
    </section>

    <section id="start" className="lp-start" aria-labelledby="start-heading">
      <div className="nr-shell lp-start-card">
        <div className="lp-start-intro">
          <p className="lp-eyebrow">{home.getStarted}</p>
          <h2 id="start-heading">{home.startWithOneFile(br, em)}</h2>
          <p className="lp-section-description">{home.theDesktopAppShipsWithRuntimeAndMedia}</p>
          {button}
          <DownloadAlternatives variants={variants} />
        </div>
        <div className="lp-steps">
          <p className="lp-eyebrow">{home.setup}</p>
          <ol>
            <li><span>01</span><div><h3>{home.installTheApp}</h3><p>{home.packagesAreAvailableForMacosAndWindows}</p></div></li>
            <li><span>02</span><div><h3>{home.writeAComposition}</h3><p>{home.createProjectOrSingleFile(() => <code>.tsx</code>)}</p></div></li>
            <li><span>03</span><div><h3>{home.openPreviewExport}</h3><p>{home.openTheFileInCelestaItReloads}</p></div></li>
          </ol>
          {docsLink('examples', home.browseTheExamples(forward))}
        </div>
      </div>
    </section>
  </div>;
}
