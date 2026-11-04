import { Assets, Audio, Composition, Easings, Font, Group, progress, useCurrentFrame } from '@celesta.studio/react';
import { TransitionSeries, computeTransitionSeries } from '@celesta.studio/transitions';
import { CUT, ChapterShell } from './components/ChapterShell';
import { Hud } from './components/Hud';
import { FPS, H, LATIN_FONTS, W, japaneseText } from './components/theme';
import { CHAPTERS } from './v0.6.0/chapters';
import { JA_STRINGS } from './v0.6.0/demos/FontStackDemo';
import { FilmContext } from './v0.6.0/film-context';
import { OPEN_LENGTH, Open } from './v0.6.0/Open';
import { OUTRO_LENGTH, Outro } from './v0.6.0/Outro';
import { PRS } from './v0.6.0/prs';
import { CHAPTER_COUNT, CHAPTER_LENGTH, OUTRO_AT, chapterAt } from './v0.6.0/timeline';

// DELTA 0.6.0: what changed from Celesta v0.5.0 to v0.6.0, in a minute.
// Twelve features, two bars each, cut to a 120 BPM score; every chapter is a
// release-notes page with a live demo of the feature drawn by the feature.
// Open this file in Celesta, or export it with:
//   python3 examples/delta/make-score-0.6.0.py
//   Celesta-export --react examples/delta/film-0.6.0.tsx examples/delta/delta-0.6.0.mp4
//
//   components/   the DELTA look, shared by every release film: theme, chapter page, HUD, diff panel, shaders
//   v0.6.0/       this release: the open, the chapters (chapters.tsx lists them, demos/ draws them), the outro
//
// The film is cut with the features it shows: chapters are joined by a
// TransitionSeries, the backgrounds are <Static> <Instances>, the diffs are
// drawn from highlightCode(), the titles track in with letterSpacing in em,
// and the outro redraws the film itself through <FreezeFrame>.

const WIPE = { type: 'wipe', from: 'right', durationInFrames: CUT, easing: Easings.easeInOutCubic } as const;

// Each scene runs CUT frames past its bar line, so every cut lands on one.
const SCENES = [
  OPEN_LENGTH,
  ...CHAPTERS.map(() => CHAPTER_LENGTH + CUT),
  OUTRO_LENGTH + CUT,
];
const series = computeTransitionSeries(SCENES.flatMap((length, i) => (
  i === 0 ? [{ durationInFrames: length }] : [{ type: WIPE.type, durationInFrames: CUT }, { durationInFrames: length }]
)));
const DURATION = series.durationInFrames;
/** The frame each chapter's cut lands on. */
const CHAPTER_AT = CHAPTERS.map((_, i) => series.sequences[i + 1].from + CUT);
// FreezeFrame and the outro find chapters through timeline.ts; keep it honest.
if (CHAPTERS.length !== CHAPTER_COUNT || CHAPTER_AT.some((at, i) => at !== chapterAt(i))
  || series.sequences[CHAPTERS.length + 1].from + CUT !== OUTRO_AT) {
  throw new Error('v0.6.0/timeline.ts does not match the cuts in film-0.6.0.tsx');
}

function Film() {
  const outro = CHAPTERS.map((chapter, i) => ({ title: chapter.title, at: CHAPTER_AT[i] + 75 }));
  return (
    <TransitionSeries>
      <TransitionSeries.Sequence durationInFrames={SCENES[0]}><Open /></TransitionSeries.Sequence>
      {CHAPTERS.flatMap((chapter, i) => [
        <TransitionSeries.Transition key={`cut-${chapter.key}`} {...WIPE} />,
        <TransitionSeries.Sequence key={chapter.key} durationInFrames={SCENES[i + 1]}>
          <ChapterShell chapter={chapter} index={i} total={CHAPTERS.length} />
        </TransitionSeries.Sequence>,
      ])}
      <TransitionSeries.Transition {...WIPE} />
      <TransitionSeries.Sequence durationInFrames={SCENES[SCENES.length - 1]}><Outro chapters={outro} /></TransitionSeries.Sequence>
    </TransitionSeries>
  );
}

function ChapterHud() {
  const frame = useCurrentFrame();
  if (frame < CHAPTER_AT[0] || frame >= OUTRO_AT) return null;
  // In after the first cut has landed, out with the wipe into the outro.
  const opacity = progress(frame, CHAPTER_AT[0], 10) * (1 - progress(frame, OUTRO_AT - CUT, CUT));
  return <Group opacity={opacity}><Hud version="0.6.0" starts={CHAPTER_AT} length={CHAPTER_LENGTH} /></Group>;
}

export default function Root() {
  return (
    <Composition width={W} height={H} fps={FPS} durationInFrames={DURATION} lang="ja-JP">
      <Assets>
        <Font src={LATIN_FONTS} />
        <Font google="Noto Sans JP" weights={[500, 700]} text={japaneseText([...PRS.map(([, title]) => title), ...JA_STRINGS])} />
      </Assets>
      <FilmContext.Provider value={Film}>
        <Film />
      </FilmContext.Provider>
      <ChapterHud />
      <Audio src="./score-0.6.0.wav" />
    </Composition>
  );
}
