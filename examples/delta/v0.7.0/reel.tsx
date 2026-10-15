import { Assets, Font, Group, Subtitles, progress, useCurrentFrame } from '@celesta/react';
import { TransitionSeries, computeTransitionSeries } from '@celesta/transitions';
import { CUT, ChapterShell } from '../components/ChapterShell';
import { japaneseFonts } from '../components/theme';
import { CAPTIONS, CAPTION_LANGUAGES } from './captions';
import { CHAPTERS } from './chapters';
import { CUTS, CUT_EASING } from './cuts';
import { Hud } from './Hud';
import { LANDSCAPE, LayoutContext } from './layout';
import type { Layout } from './layout';
import { MORE_STRINGS, More } from './More';
import { Open } from './Open';
import { Outro } from './Outro';
import { PRS } from './prs';
import { ShortChapter } from './ShortChapter';
import { CHAPTER_COUNT, CHAPTER_LENGTHS, OPEN_LENGTH, OUTRO_AT, OUTRO_LENGTH, chapterAt } from './timeline';

// The cut both entries play: film-0.7.0.tsx at 16:9, shorts-0.7.0.tsx at
// 9:16. Same scenes, same transitions, same score; only the layout differs.

// Each scene runs CUT frames past its bar line, so every cut lands on one.
const SCENES = [OPEN_LENGTH, ...CHAPTER_LENGTHS.map((length) => length + CUT), OUTRO_LENGTH + CUT];
const series = computeTransitionSeries(SCENES.flatMap((length, i) => (
  i === 0 ? [{ durationInFrames: length }] : [{ type: CUTS[i - 1].type, durationInFrames: CUT }, { durationInFrames: length }]
)));
export const DURATION = series.durationInFrames;
/** The frame each chapter's cut lands on. */
const CHAPTER_AT = CHAPTER_LENGTHS.map((_, i) => series.sequences[i + 1].from + CUT);
// FreezeFrame and the outro find chapters through timeline.ts; keep it honest.
if (CHAPTERS.length + 1 !== CHAPTER_COUNT || CUTS.length !== CHAPTER_COUNT + 1
  || CHAPTER_AT.some((at, i) => at !== chapterAt(i)) || series.sequences[CHAPTER_COUNT + 1].from + CUT !== OUTRO_AT) {
  throw new Error('v0.7.0/timeline.ts does not match the cuts in v0.7.0/reel.tsx');
}
/** The outro's last frame with everything in: the poster. */
export const POSTER_AT = DURATION - 30;

function Scene({ index, portrait }: { index: number; portrait: boolean }) {
  if (index >= CHAPTERS.length) return <More />;
  const Page = portrait ? ShortChapter : ChapterShell;
  return <Page chapter={CHAPTERS[index]} index={index} total={CHAPTERS.length} />;
}

/** The whole cut, laid out for one canvas. */
export function Reel({ layout }: { layout: Layout }) {
  const outro = [
    ...CHAPTERS.map((chapter, i) => ({ title: chapter.title, at: CHAPTER_AT[i] + 100 })),
    { title: 'MORE', at: CHAPTER_AT[CHAPTERS.length] + 20 },
  ];
  return (
    <LayoutContext.Provider value={layout}>
      <TransitionSeries width={layout.width} height={layout.height}>
        <TransitionSeries.Sequence durationInFrames={SCENES[0]}><Open chapters={CHAPTERS.length} /></TransitionSeries.Sequence>
        {CHAPTER_LENGTHS.flatMap((_, i) => [
          <TransitionSeries.Transition key={`cut-${i}`} type={CUTS[i].type} from={CUTS[i].from}
            durationInFrames={CUT} easing={CUT_EASING} />,
          <TransitionSeries.Sequence key={`scene-${i}`} durationInFrames={SCENES[i + 1]}>
            <Scene index={i} portrait={layout.portrait} />
          </TransitionSeries.Sequence>,
        ])}
        <TransitionSeries.Transition type={CUTS[CHAPTER_COUNT].type} from={CUTS[CHAPTER_COUNT].from}
          durationInFrames={CUT} easing={CUT_EASING} />
        <TransitionSeries.Sequence durationInFrames={SCENES[SCENES.length - 1]}><Outro chapters={outro} /></TransitionSeries.Sequence>
      </TransitionSeries>
    </LayoutContext.Provider>
  );
}

/**
 * The 16:9 film. Every <FreezeFrame> of "the film" redraws this one, in
 * either entry: the short's thumbnails and replays are the film's frames.
 */
export function Film() {
  return <Reel layout={LANDSCAPE} />;
}

/** Top-right progress and the score's meter, over the twelve chapter pages. */
export function ChapterHud({ layout }: { layout: Layout }) {
  const frame = useCurrentFrame();
  // MORE and the outro are full-bleed.
  const end = CHAPTER_AT[CHAPTERS.length];
  if (frame < CHAPTER_AT[0] || frame >= end) return null;
  const opacity = progress(frame, CHAPTER_AT[0], 10) * (1 - progress(frame, end - CUT, CUT));
  // The short keeps it inside the safe area, under the platforms' top bar.
  const place = layout.portrait ? { right: layout.width - layout.safe.right - 24, y: layout.safe.top + 40 } : {};
  return (
    <Group opacity={opacity}>
      <Hud version="0.7.0" starts={CHAPTER_AT.slice(0, -1)} lengths={CHAPTER_LENGTHS.slice(0, -1)} {...place} />
    </Group>
  );
}

// Every language at once: --subtitles 'delta-0.7.0.{lang}.srt' writes one
// file per language.
export function Captions() {
  return (
    <>
      {CAPTION_LANGUAGES.map((lang) => <Subtitles key={lang} lang={lang} cues={CAPTIONS[lang]} />)}
    </>
  );
}

const JA_STRINGS = [...PRS.map(([, title]) => title), ...MORE_STRINGS];

export function Fonts() {
  // Latin faces by family name (#341); Japanese subset to the film's glyphs.
  return (
    <Assets>
      <Font google="Bebas Neue" />
      <Font google="Space Grotesk" weights={[400, 500, 700]} />
      <Font google="JetBrains Mono" weights={[400, 700]} />
      <Font src={japaneseFonts(JA_STRINGS)} />
    </Assets>
  );
}
