import { Assets, Audio, Character, Composition, Font, Rect, Sequence } from '@celesta/react';
import { akane, MOUTH, PSD, poseLayers } from './character';
import { END, FPS, H, INK, PAPER, W } from './constants';
import { Characters } from './scenes/Characters';
import { Closing } from './scenes/Closing';
import { Delivery } from './scenes/Delivery';
import { Identity } from './scenes/Identity';
import { Motion } from './scenes/Motion';
import { Opening } from './scenes/Opening';
import { Punctuation } from './scenes/Punctuation';
import { Rhythm } from './scenes/Rhythm';
import { Scrubbing } from './scenes/Scrubbing';
import { Source } from './scenes/Source';
import { Typography } from './scenes/Typography';

// PRISM — a Celesta product film. 48 seconds / 120 BPM / 30 fps.
// Each chapter is a real React component, each image a pure function of time.
//
//   scenes/        one file per chapter, in playback order (see the timeline below)
//   components/    Label (text), Line, Prism (the recurring motif), Rails (page chrome)
//   character.ts   Akane's PSD / voice assets and prepare()    constants.ts, math.ts   palette, helpers

// Loads the PSD pose preset and the lip-sync track before the first frame.
export { prepare } from './character';

export default function Film() {
  return <Composition width={W} height={H} fps={FPS} durationInFrames={END}>
    <Assets>
      <Font src="./assets/fonts/BebasNeue-Regular.ttf" />
      <Font src="./assets/fonts/IBMPlexMono-Regular.ttf" />
      <Character ref={akane} name="Kotonoha Akane" portrait={{ type: 'psd', src: PSD,
        layers: poseLayers, lipSync: {
          a: `${MOUTH}/あいうえお/*あ`, i: `${MOUTH}/あいうえお/*い`,
          u: `${MOUTH}/あいうえお/*う`, e: `${MOUTH}/あいうえお/*え`,
          o: `${MOUTH}/あいうえお/*お`, closed: `${MOUTH}/*-`,
        },
      }} subtitle={{ x: 80, y: 824, maxWidth: 860,
        style: { fontFamily: 'IBM Plex Mono', fontSize: 28,
          fill: { type: 'solid', color: PAPER } } }} />
    </Assets>
    <Rect width={W} height={H} fill={INK} />
    <Audio src="./assets/score.wav" />
    {/* Timeline: each chapter's start frame and length. */}
    <Sequence from={0} durationInFrames={60}><Opening /></Sequence>
    <Sequence from={60} durationInFrames={120}><Identity /></Sequence>
    <Sequence from={180} durationInFrames={150}><Source /></Sequence>
    <Sequence from={330} durationInFrames={150}><Motion /></Sequence>
    <Sequence from={480} durationInFrames={150}><Rhythm /></Sequence>
    <Sequence from={630} durationInFrames={150}><Typography /></Sequence>
    <Sequence from={780} durationInFrames={180}><Characters /></Sequence>
    <Sequence from={960} durationInFrames={150}><Scrubbing /></Sequence>
    <Sequence from={1110} durationInFrames={90}><Punctuation /></Sequence>
    <Sequence from={1200} durationInFrames={120}><Delivery /></Sequence>
    <Sequence from={1320} durationInFrames={120}><Closing /></Sequence>
  </Composition>;
}
