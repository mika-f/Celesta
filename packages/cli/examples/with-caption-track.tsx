import { Composition, Font, Group, Rect, Sequence, Text, interpolate } from '@celesta.studio/react';
import { CaptionTrack } from '@celesta.studio/components';
import type { CaptionCue, CaptionRenderProps } from '@celesta.studio/components';

// Captions from data alone: no <Character>, voice, or audio. The bottom track
// uses the default band; the top one draws each cue itself with `render`,
// on the same cue clock (a progress bar under the text runs over each cue).
// 150–180 is a gap with no caption; the last cue wraps to two lines.

const W = 1280, H = 720, FPS = 30;

const cues: CaptionCue[] = [
  { id: 'hello', from: 0, durationInFrames: 60, text: 'こんにちは。データだけで字幕を出します。' },
  { id: 'swap', from: 60, durationInFrames: 90, text: '続けて並ぶ字幕は、まばたきせずに切り替わります。',
    emphasis: [{ start: 9, end: 13, style: 'key' }] },
  { id: 'wrap', from: 180, durationInFrames: 120,
    text: '長い字幕は最大幅で折り返し、二行に収まらなければ文字を小さくします。Mixed English text wraps too.',
    emphasis: [{ start: 9, end: 11, style: 'key' }, { start: 14, end: 16, style: 'warn' }] },
];

const emphasisStyles = { key: { color: '#ffd166', fontWeight: 700 }, warn: { color: '#ff6b6b', fontWeight: 700 } };

/** A caption with its id, drawn from what the track measured, and the cue's progress. */
function Labelled({ cue, content, style, metrics, maxWidth, frame, durationInFrames }: CaptionRenderProps) {
  const width = Math.max(metrics.width, 240);
  return <Group x={(W - width) / 2} y={60}>
    <Text y={-12} style={{ fontFamily: 'JetBrains Mono', fontSize: 18, fill: { type: 'solid', color: '#8b8f9c' } }}>
      {`#${cue.id}  ${frame + 1}/${durationInFrames}`}
    </Text>
    <Text x={(width - maxWidth) / 2} y={metrics.ascent + 8} anchorY="baseline" maxWidth={maxWidth} style={style}>{content}</Text>
    <Rect y={metrics.height + 20} width={width} height={4} fill="#3a3e4a" />
    <Rect y={metrics.height + 20} width={width * interpolate(frame, [0, durationInFrames - 1], [0, 1])} height={4} fill="#28a34a" />
  </Group>;
}

export default function Root() {
  return <Composition width={W} height={H} fps={FPS} durationInFrames={330} lang="ja-JP">
    <Font src="https://fonts.googleapis.com/css2?family=Noto+Sans+JP:wght@500;700" />
    <Font src="https://fonts.googleapis.com/css2?family=JetBrains+Mono:wght@400" />
    <Rect width={W} height={H} fill="#1d3b53" />
    <CaptionTrack cues={cues} emphasisStyles={emphasisStyles} fontFamily="Noto Sans JP" maxWidth={900} minFontSize={26} />
    {/* The same cues in a Sequence starting at frame 0, drawn by `render`; not declared twice. */}
    <Sequence from={0}>
      <CaptionTrack cues={cues} emphasisStyles={emphasisStyles} fontFamily="Noto Sans JP" fontSize={28} maxWidth={760}
        subtitles={false} enter="fade-down" exit="fade" render={Labelled} />
    </Sequence>
  </Composition>;
}
