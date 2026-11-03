import * as React from 'react';
import { Assets, Composition, Font, Rect, Sequence, Text } from '@celesta.studio/react';
import { CaptionTrack, PatternBackground } from '@celesta.studio/components';
import type { CaptionCue, CaptionTrackProps } from '@celesta.studio/components';

// Captions lit word by word, as vertical short videos set them. Three
// takes on one <CaptionTrack>:
//
// 1. English with word timings, as a speech-to-text transcript gives them:
//    a band slides to the word being spoken, three words a page.
// 2. Japanese with per-morpheme timings, as a voice engine gives them:
//    `wordUnit="phrase"` joins them into phrases (文節), lit one at a time.
// 3. Japanese with no timings at all: the cue's frames are shared out by
//    length, a line a page.

const W = 1080, H = 1920, FPS = 30;
const BG = '#121317', INK = '#f5f3ee', MUTED = '#8a8d98', ACCENT = '#ffd166';
const FONT = 'Noto Sans JP';
const MONO = ['JetBrains Mono', 'Noto Sans JP'];

// 1. English, word timings from a transcript (frames at 30 fps).
const english: CaptionCue[] = [
  {
    from: 0, durationInFrames: 75, text: 'Captions that follow the voice,',
    words: [
      { text: 'Captions', from: 4 }, { text: 'that', from: 22 }, { text: 'follow', from: 32 },
      { text: 'the', from: 46 }, { text: 'voice', from: 54 },
    ],
  },
  {
    from: 75, durationInFrames: 75, text: 'one word at a time.',
    words: [
      { text: 'one', from: 78 }, { text: 'word', from: 90 }, { text: 'at', from: 104 },
      { text: 'a', from: 112 }, { text: 'time', from: 120, durationInFrames: 24 },
    ],
  },
];

// 2. Japanese, a timing per morpheme. "、" and "。" go with the word before.
const japanese: CaptionCue[] = [{
  from: 0, durationInFrames: 150, text: '話している言葉を、文節ごとに追いかけます。',
  words: [
    { text: '話し', from: 6 }, { text: 'て', from: 18 }, { text: 'いる', from: 24 },
    { text: '言葉', from: 38 }, { text: 'を', from: 52 },
    { text: '文節', from: 66 }, { text: 'ごと', from: 80 }, { text: 'に', from: 90 },
    { text: '追いかけ', from: 100 }, { text: 'ます', from: 124 },
  ],
}];

// 3. Japanese, no timings.
const untimed: CaptionCue[] = [
  { from: 0, durationInFrames: 150, text: '時刻のない字幕も、文字の長さに合わせて一行ずつ進みます。' },
];

/** What every take shares: centred over the picture, clear of the platform's UI. */
const shared: Omit<CaptionTrackProps, 'cues'> = {
  safeArea: 'vertical-social', position: 'middle', fontFamily: FONT, color: INK,
  style: { fontWeight: 900 }, maxWidth: 760, margin: 0, padding: 0, background: null, exit: null,
};

const takes: { label: string; track: CaptionTrackProps }[] = [
  {
    label: 'words + highlight.background\npaging={{ maxWords: 3 }}',
    track: {
      ...shared, cues: english, fontSize: 104, lineHeight: 1.3,
      highlight: { color: '#111111', background: ACCENT, scale: 1.08, upcomingColor: '#f5f3ee99', cornerRadius: 0 },
      paging: { maxWords: 3 },
    },
  },
  {
    label: 'words + wordUnit="phrase"\nhighlight.scale',
    track: {
      ...shared, cues: japanese, fontSize: 96, lineHeight: 1.35, wordUnit: 'phrase', maxLines: 2,
      highlight: { color: ACCENT, scale: 1.18, upcomingColor: '#f5f3ee66' },
    },
  },
  {
    label: 'no words: frames by length\nmaxLines={1}',
    track: {
      ...shared, cues: untimed, fontSize: 88, wordUnit: 'phrase', maxLines: 1,
      highlight: { color: BG, background: ACCENT, spokenColor: MUTED, cornerRadius: 0 },
    },
  },
];
const TAKE = 150;

export default function WordCaptions() {
  return (
    <Composition width={W} height={H} fps={FPS} durationInFrames={takes.length * TAKE} lang="ja-JP">
      <Assets>
        <Font src="https://fonts.googleapis.com/css2?family=Noto+Sans+JP:wght@400;900" />
        <Font src="https://fonts.googleapis.com/css2?family=JetBrains+Mono:wght@400" />
      </Assets>
      <PatternBackground background={BG} color="#ffffff" opacity={0.06} spacing={48} size={4} speedY={24} />
      {takes.map(({ label, track }, index) => (
        <Sequence key={index} from={index * TAKE} durationInFrames={TAKE}>
          <Rect x={72} y={240} width={8} height={88} fill={ACCENT} />
          <Text x={104} y={240} style={{ fontFamily: MONO, fontSize: 32, lineHeight: 44, fill: { type: 'solid', color: MUTED } }}>
            {label}
          </Text>
          <CaptionTrack {...track} />
        </Sequence>
      ))}
    </Composition>
  );
}
