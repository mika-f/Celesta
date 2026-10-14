import * as fs from 'node:fs';
import * as path from 'node:path';

import {
  Audio,
  Composition,
  Easings,
  Font,
  Group,
  Image,
  Rect,
  Sequence,
  defineProjectProperties,
  frameKeyframes,
  getProjectProperty,
  progress,
  useCurrentFrame,
} from '@celesta/react';
import type { Paint } from '@celesta/react';
import { mediaDurationInFrames, preloadMedia } from '@celesta/media-utils';
import { useProjectProperty } from '@celesta/project';
import { TextBox } from '@celesta/text';
import { TransitionSeries, computeTransitionSeries } from '@celesta/transitions';
import type { TransitionSeriesItem, TransitionSeriesType } from '@celesta/transitions';

// A photo slideshow: a title card, then one photo per slide with a slow zoom
// and its caption along the bottom, joined by transitions, over music.
//
// - props.json holds the values this project renders with: the title and
//   subtitle, the accent color, the format (16:9 / 9:16), the transition
//   (crossfade, slide, wipe, iris, …), the seconds each slide stays, and the slides
//   and music files.
// - slides.json lists the slides: an array of { "image", "caption"? }.
//   Image paths resolve from the folder holding slides.json, and the video
//   gets longer or shorter with the number of slides.
// - assets/ holds the sample photos and music. Replace them with your own,
//   or point slides.json and the `music` property elsewhere. Set `music` to
//   "" for a silent slideshow.
//
// Every layout number and motion timing is in the constants block below.

// ---- Constants --------------------------------------------------------------

const FPS = 30;
// Timing. The title card lasts TITLE_SECONDS; each transition overlaps the
// scenes on either side by TRANSITION_FRAMES. The title and subtitle fade in
// over TITLE_IN, rising TITLE_RISE pixels. A caption starts once its slide's
// transition in has finished and fades in over CAPTION_IN, rising
// CAPTION_RISE pixels. The music fades out over the last MUSIC_FADE_SECONDS.
const TITLE_SECONDS = 3;
const TRANSITION_FRAMES = 20;
const TITLE_IN = 24;
const TITLE_RISE = 24;
const CAPTION_IN = 18;
const CAPTION_RISE = 16;
const MUSIC_FADE_SECONDS = 1;
// The edge a sliding or wiping slide enters from.
const TRANSITION_FROM = 'right';
// Each photo's scale grows from 1 to 1 + ZOOM over its slide, drifting
// sideways (alternating direction) by PAN of the room the zoom adds at the
// edges, so the frame never shows past the photo.
const ZOOM = 0.08;
const PAN = 0.6;

const FONT = 'Noto Sans JP';
const FONT_SRC = 'https://fonts.googleapis.com/css2?family=Noto+Sans+JP:wght@400;700';

const COLORS = {
  // The title card's background, top to bottom.
  backgroundTop: '#1a1d26',
  backgroundBottom: '#0d0f14',
  text: '#ffffff',
  // The darkening behind captions, clear at its top and darkest at the
  // bottom of the frame.
  scrimTop: '#00000000',
  scrimBottom: '#000000b3',
};
// A soft shadow under caption text, for bright photos.
const CAPTION_SHADOW = { color: '#00000099', blur: 12, offsetX: 0, offsetY: 2 };

// Layout, in pixels. Each format sets the canvas and the sizes that differ
// between a wide and a tall frame. Text keeps MARGIN from the sides; a
// caption's last line sits `captionBottom` above the bottom edge (higher in
// 9:16, clear of the controls phones draw over vertical video).
type Format = '16:9' | '9:16';
interface FormatLayout {
  width: number;
  height: number;
  titleSize: number;
  subtitleSize: number;
  captionSize: number;
  captionBottom: number;
}
const FORMATS: Record<Format, FormatLayout> = {
  '16:9': { width: 1920, height: 1080, titleSize: 120, subtitleSize: 52, captionSize: 56, captionBottom: 80 },
  '9:16': { width: 1080, height: 1920, titleSize: 104, subtitleSize: 52, captionSize: 64, captionBottom: 240 },
};
const MARGIN = 80;

// Text that does not fit shrinks, down to its minimum, wrapping onto at most
// its line count; text still too long at the minimum is cut off.
const TITLE_MIN_SIZE = 56;
const TITLE_MAX_LINES = 2;
const SUBTITLE_MIN_SIZE = 28;
const CAPTION_MIN_SIZE = 36;
const CAPTION_MAX_LINES = 2;
// The height of a line, as a multiple of its font size.
const LINE_HEIGHT = 1.3;
// The accent rule between the title (above it) and the subtitle (below),
// centered on the title card, with RULE_GAP to each.
const RULE_WIDTH = 120;
const RULE_HEIGHT = 6;
const RULE_GAP = 36;
// The caption scrim reaches SCRIM_EXTRA above the caption's room.
const SCRIM_EXTRA = 160;

defineProjectProperties({
  title: { type: 'string', label: 'Title', defaultValue: '旅の記録' },
  subtitle: { type: 'string', label: 'Subtitle', defaultValue: '2026 秋' },
  format: { type: 'select', label: 'Format', defaultValue: '16:9', options: ['16:9', '9:16'] },
  accent: { type: 'color', label: 'Accent', defaultValue: '#f2c14e' },
  transition: {
    type: 'select',
    label: 'Transition',
    defaultValue: 'crossfade',
    options: ['crossfade', 'slide', 'wipe', 'iris', 'clock-wipe', 'zoom', 'dissolve', 'blur', 'flip', 'cube'],
  },
  secondsPerSlide: { type: 'number', label: 'Seconds per slide', defaultValue: 4 },
  slides: { type: 'path', label: 'Slides', defaultValue: './slides.json' },
  music: { type: 'path', label: 'Music', defaultValue: './assets/music.wav' },
});

interface Slide {
  image: string;
  caption: string;
}

const REMOTE = /^https?:\/\//i;

let slides: Slide[] = [];
let slideFrames = 0;
// The music's length, to repeat it when the slideshow runs longer.
let musicFrames: number | undefined;

// `getProjectProperty` sees the same values the render does, so the slides
// read here and the duration computed from them always match.
export async function prepare(): Promise<void> {
  const file = getProjectProperty<string>('slides');
  // A `path` property may also be an http(s) URL, which is fetched instead.
  const text = REMOTE.test(file) ? await (await fetch(file)).text() : await fs.promises.readFile(file, 'utf8');
  let parsed: unknown;
  try {
    parsed = JSON.parse(text);
  } catch (error) {
    throw new Error(`${file}: ${error instanceof Error ? error.message : String(error)}`);
  }
  if (!Array.isArray(parsed) || parsed.length === 0) {
    throw new Error(`${file}: slides must be a non-empty array of { image, caption? }`);
  }
  slides = parsed.map((entry: unknown, index) => {
    const { image, caption } = (entry ?? {}) as { image?: unknown; caption?: unknown };
    if (typeof image !== 'string' || image === '') {
      throw new Error(`${file}: slide ${index} has no image`);
    }
    if (caption !== undefined && typeof caption !== 'string') {
      throw new Error(`${file}: slide ${index} has a caption that is not a string`);
    }
    return { image: resolveImage(file, image), caption: caption ?? '' };
  });
  slides.forEach(({ image }, index) => {
    if (!REMOTE.test(image) && !fs.existsSync(image)) {
      throw new Error(`${file}: slide ${index} image ${image} does not exist`);
    }
  });

  // A slide holds its transition in and its transition out.
  const seconds = getProjectProperty<number>('secondsPerSlide');
  slideFrames = Math.round(seconds * FPS);
  if (slideFrames < 2 * TRANSITION_FRAMES) {
    throw new Error(`secondsPerSlide is ${seconds}; it must be at least ${((2 * TRANSITION_FRAMES) / FPS).toFixed(2)}`);
  }

  const music = getProjectProperty<string>('music');
  musicFrames = music === '' ? undefined : mediaDurationInFrames(await preloadMedia(music), FPS);
}

// Relative image paths resolve from the slides file's folder (or URL).
function resolveImage(file: string, image: string): string {
  if (REMOTE.test(file)) return new URL(image, file).href;
  return REMOTE.test(image) ? image : path.resolve(path.dirname(file), image);
}

// The scenes and transitions of the slideshow, for computeTransitionSeries to
// size the composition; <Slideshow> renders the same sequence.
function seriesItems(type: TransitionSeriesType): TransitionSeriesItem[] {
  return [
    { durationInFrames: TITLE_SECONDS * FPS },
    ...slides.flatMap((): TransitionSeriesItem[] => [
      { type, durationInFrames: TRANSITION_FRAMES },
      { durationInFrames: slideFrames },
    ]),
  ];
}

function useLayout() {
  return FORMATS[useProjectProperty<Format>('format')];
}

function vertical(height: number, top: string, bottom: string): Paint {
  return {
    type: 'linear',
    start: { x: 0, y: 0 },
    end: { x: 0, y: height },
    stops: [{ offset: 0, color: top }, { offset: 1, color: bottom }],
  };
}

function TitleCard() {
  const frame = useCurrentFrame();
  const { width, height, titleSize, subtitleSize } = useLayout();
  const accent = useProjectProperty<string>('accent');
  // The card is fully on screen from frame 0; it has no transition in.
  const enter = progress(frame, 0, TITLE_IN, Easings.easeOutCubic);
  const rise = TITLE_RISE * (1 - enter);
  const center = height / 2;
  const textWidth = width - 2 * MARGIN;
  const titleHeight = TITLE_MAX_LINES * titleSize * LINE_HEIGHT;
  return (
    <>
      <Rect width={width} height={height} fill={vertical(height, COLORS.backgroundTop, COLORS.backgroundBottom)} />
      {/* The title grows upward from the rule, the subtitle hangs below it. */}
      <TextBox
        x={MARGIN}
        y={center - RULE_GAP - titleHeight + rise}
        width={textWidth}
        height={titleHeight}
        minFontSize={TITLE_MIN_SIZE}
        maxFontSize={titleSize}
        maxLines={TITLE_MAX_LINES}
        lineHeight={LINE_HEIGHT}
        verticalAlign="bottom"
        opacity={enter}
        style={{ fontFamily: FONT, fontWeight: 700, align: 'center', fill: { type: 'solid', color: COLORS.text } }}
      >
        {useProjectProperty<string>('title')}
      </TextBox>
      <Rect
        x={width / 2}
        y={center}
        anchorX={0.5}
        anchorY={0.5}
        width={RULE_WIDTH}
        height={RULE_HEIGHT}
        cornerRadius={RULE_HEIGHT / 2}
        scaleX={enter}
        fill={accent}
      />
      <TextBox
        x={MARGIN}
        y={center + RULE_GAP + rise}
        width={textWidth}
        height={subtitleSize * LINE_HEIGHT}
        minFontSize={SUBTITLE_MIN_SIZE}
        maxFontSize={subtitleSize}
        maxLines={1}
        lineHeight={LINE_HEIGHT}
        opacity={enter}
        style={{ fontFamily: FONT, align: 'center', fill: { type: 'solid', color: accent } }}
      >
        {useProjectProperty<string>('subtitle')}
      </TextBox>
    </>
  );
}

function SlideScene({ slide, index }: { slide: Slide; index: number }) {
  // Counts from the first frame of this slide's transition in.
  const frame = useCurrentFrame();
  const { width, height, captionSize, captionBottom } = useLayout();
  const scale = 1 + ZOOM * Math.min(1, frame / slideFrames);
  const drift = (index % 2 === 0 ? 1 : -1) * PAN * ((scale - 1) * width) / 2;
  const captionHeight = CAPTION_MAX_LINES * captionSize * LINE_HEIGHT;
  const captionTop = height - captionBottom - captionHeight;
  const scrimHeight = height - captionTop + SCRIM_EXTRA;
  const enter = progress(frame, TRANSITION_FRAMES, CAPTION_IN, Easings.easeOutCubic);
  // The zoomed photo overhangs the frame; clip it there, or a sliding slide
  // shows the overhang beside it.
  return (
    <Group clip={{ width, height }}>
      <Image
        src={slide.image}
        x={width / 2 + drift}
        y={height / 2}
        anchorX={0.5}
        anchorY={0.5}
        width={width}
        height={height}
        fit="cover"
        scale={scale}
      />
      {slide.caption !== '' && (
        <>
          <Rect y={height - scrimHeight} width={width} height={scrimHeight} fill={vertical(scrimHeight, COLORS.scrimTop, COLORS.scrimBottom)} />
          <TextBox
            x={MARGIN}
            y={captionTop + CAPTION_RISE * (1 - enter)}
            width={width - 2 * MARGIN}
            height={captionHeight}
            minFontSize={CAPTION_MIN_SIZE}
            maxFontSize={captionSize}
            maxLines={CAPTION_MAX_LINES}
            lineHeight={LINE_HEIGHT}
            verticalAlign="bottom"
            opacity={enter}
            shadow={CAPTION_SHADOW}
            style={{ fontFamily: FONT, fontWeight: 700, fill: { type: 'solid', color: COLORS.text } }}
          >
            {slide.caption}
          </TextBox>
        </>
      )}
    </Group>
  );
}

// The music from the start, repeated when it is shorter than the slideshow,
// fading out over the last MUSIC_FADE_SECONDS.
function Music({ durationInFrames }: { durationInFrames: number }) {
  const music = useProjectProperty<string>('music');
  if (music === '') return null;
  const loop = musicFrames ?? durationInFrames;
  const fadeStart = durationInFrames - MUSIC_FADE_SECONDS * FPS;
  return (
    <>
      {Array.from({ length: Math.ceil(durationInFrames / loop) }, (_, index) => {
        const from = index * loop;
        const volume = frameKeyframes(
          [
            { frame: fadeStart, value: 1 },
            { frame: durationInFrames, value: 0 },
          ],
          { fps: FPS, origin: from },
        );
        return (
          <Sequence key={index} from={from} durationInFrames={Math.min(loop, durationInFrames - from)}>
            <Audio src={music} volume={volume} />
          </Sequence>
        );
      })}
    </>
  );
}

export default function Slideshow() {
  const { width, height } = useLayout();
  const transition = useProjectProperty<TransitionSeriesType>('transition');
  // The scenes overlap by their transitions, so the total is shorter than
  // the scenes added up; computeTransitionSeries works it out.
  const { durationInFrames } = computeTransitionSeries(seriesItems(transition));
  return (
    <Composition width={width} height={height} fps={FPS} durationInFrames={durationInFrames}>
      <Font src={FONT_SRC} />
      <TransitionSeries>
        <TransitionSeries.Sequence durationInFrames={TITLE_SECONDS * FPS}>
          <TitleCard />
        </TransitionSeries.Sequence>
        {slides.flatMap((slide, index) => [
          <TransitionSeries.Transition
            key={`transition-${index}`}
            type={transition}
            durationInFrames={TRANSITION_FRAMES}
            from={TRANSITION_FROM}
            easing={Easings.easeInOutCubic}
          />,
          <TransitionSeries.Sequence key={`slide-${index}`} durationInFrames={slideFrames}>
            <SlideScene slide={slide} index={index} />
          </TransitionSeries.Sequence>,
        ])}
      </TransitionSeries>
      <Music durationInFrames={durationInFrames} />
    </Composition>
  );
}
