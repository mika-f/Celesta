import * as React from 'react';
import {
  Assets,
  Composition,
  Font,
  Group,
  Rect,
  Sequence,
  Text,
  formatSubtitles,
  parseSubtitles,
  useCurrentFrame,
} from '@celesta.studio/react';
import { Character, CharacterView, Dialogue } from '@celesta.studio/character';
import { CaptionTrack } from '@celesta.studio/components';
import type { CaptionCue, CaptionTrackProps } from '@celesta.studio/components';
import type { AssetReference, SubtitleCue } from '@celesta.studio/react';
import type { CharacterViewReference, SubtitleRenderProps } from '@celesta.studio/character';
import captions from './captions.srt';

// Captions from an SRT file. parseSubtitles() turns the file into frame-timed
// cues, and <CaptionTrack> draws the one on each frame in a band and declares
// them all for `--subtitles`. A <Dialogue> line fills the gap the file leaves
// at 8–10 s; its subtitle.render draws the same band with a one-cue track,
// and it goes into the exported file too.

const W = 1920, H = 1080, FPS = 30;
const INK = '#f4f1ea', MUTED = '#8b8f9c', ACCENT = '#28a34a', LINE = '#3a3e4a', BG = '#16181f';
const MONO = ['JetBrains Mono', 'Noto Sans JP'];

const parsed = parseSubtitles(captions, { fps: FPS });
// The file has no markup for color; emphasis is added to cue 2 here, by grapheme cluster.
const cues: CaptionCue[] = parsed.map((cue, index) => index === 1
  ? { ...cue, emphasis: [{ start: 0, end: 3, style: 'format' }, { start: 6, end: 12, style: 'format' }] }
  : cue);
const DIALOGUE = { from: 8 * FPS, durationInFrames: 2 * FPS };
const DURATION = 12 * FPS;

/** The look every caption shares: the file's cues and the dialogue line. */
const band: Omit<CaptionTrackProps, 'cues'> = {
  fontFamily: 'Noto Sans JP', fontSize: 52, lineHeight: 72 / 52, color: INK, maxWidth: 1400,
  padding: { left: 48, right: 48, top: 20, bottom: 20 }, cornerRadius: 0, margin: { bottom: 28 },
  enter: { type: 'fade', durationInFrames: 7 }, exit: null,
  emphasisStyles: { format: { color: '#5ccf7a', fontWeight: 700 } },
};

/** The cue as the file holds it, written back with formatSubtitles(). */
function SourceBlock({ cue, index }: { cue: SubtitleCue; index: number }) {
  const block = formatSubtitles([cue], { fps: FPS, format: 'srt' }).replace(/^1\n/, `${index + 1}\n`);
  return (
    <Text x={160} y={300} style={{ fontFamily: MONO, fontSize: 40, lineHeight: 60, fill: { type: 'solid', color: INK } }}>
      {block.trimEnd()}
    </Text>
  );
}

/** Every cue as a block on one strip, with the playhead. */
function Timeline() {
  const frame = useCurrentFrame();
  const x = (at: number) => 160 + (at / DURATION) * 1600;
  const block = (from: number, duration: number, color: string, key: string) => (
    <Rect key={key} x={x(from)} y={760} width={x(from + duration) - x(from) - 4} height={28} fill={color} />
  );
  const active = (from: number, duration: number) => frame >= from && frame < from + duration;
  return (
    <Group>
      <Rect x={160} y={772} width={1600} height={4} fill={LINE} />
      {parsed.map((cue, index) =>
        block(cue.from, cue.durationInFrames, active(cue.from, cue.durationInFrames) ? ACCENT : MUTED, `cue-${index}`))}
      {block(DIALOGUE.from, DIALOGUE.durationInFrames, active(DIALOGUE.from, DIALOGUE.durationInFrames) ? ACCENT : LINE, 'dialogue')}
      <Rect x={x(frame) - 2} y={740} width={4} height={68} fill={INK} />
    </Group>
  );
}

const komugi = React.createRef<AssetReference>();
const komugiView = React.createRef<CharacterViewReference>();

export default function Captions() {
  return (
    <Composition width={W} height={H} fps={FPS} durationInFrames={DURATION} lang="ja-JP">
      <Assets>
        <Font src="https://fonts.googleapis.com/css2?family=JetBrains+Mono:wght@400" />
        <Font src="https://fonts.googleapis.com/css2?family=Noto+Sans+JP:wght@400;500" />
        <Character ref={komugi} name="komugi"
          portrait={{
            defaultExpression: 'smile',
            expressions: { smile: '../assets/dialogue-demo/portraits/komugi-smile.png' },
          }}
          subtitle={{
            // The line's own window as one cue; <Dialogue> already puts it in the subtitle file.
            render: ({ text, durationInFrames }: SubtitleRenderProps) =>
              <CaptionTrack {...band} subtitles={false} cues={[{ from: 0, durationInFrames, text }]} />,
          }} />
      </Assets>
      <Rect width={W} height={H} fill={BG} />
      <Text x={160} y={160} style={{ fontFamily: MONO, fontSize: 32, fill: { type: 'solid', color: MUTED } }}>
        captions.srt
      </Text>
      <Timeline />

      {parsed.map((cue, index) => (
        <Sequence key={index} from={cue.from} durationInFrames={cue.durationInFrames}>
          <SourceBlock cue={cue} index={index} />
        </Sequence>
      ))}
      <CaptionTrack {...band} cues={cues} />

      <Sequence {...DIALOGUE}>
        <Text x={160} y={300} style={{ fontFamily: MONO, fontSize: 40, fill: { type: 'solid', color: INK } }}>
          {'<Dialogue character={komugi}>'}
        </Text>
        <CharacterView ref={komugiView} character={komugi} x={1360} y={300} />
        <Dialogue character={komugiView}>キャラクターのセリフも、同じファイルに入るよ。</Dialogue>
      </Sequence>
    </Composition>
  );
}
