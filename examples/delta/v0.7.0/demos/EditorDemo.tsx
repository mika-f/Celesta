import { useContext } from 'react';
import { Easings, FreezeFrame, Group, Rect, Text, interpolate, progress } from '@celesta.studio/react';
import { Circle } from '@celesta.studio/shapes';
import type { DemoProps } from '../../components/ChapterShell';
import { STAGE, enter } from '../../components/ChapterShell';
import { C, FONT, W, solid } from '../../components/theme';
import { FilmContext } from '../film-context';
import { scoreAt } from '../score';
import { BAR, BEAT, DURATION, chapterAt } from '../timeline';

// The editor, drawn to resemble it: this film in the preview, its videos and
// its score on the timeline (the waveform is the score's loudness), a pinch
// that zooms the tracks, a click that opens the selected layer's source, and
// the Problems tab.
const mono = (size: number, color: string, weight = 400) =>
  ({ fontFamily: FONT.mono, fontSize: size, fontWeight: weight, fill: solid(color) }) as const;
const PREVIEW_AT = chapterAt(4) + 120;
const PV = { width: 480, height: 270 };

function Preview({ frame }: { frame: number }) {
  const Film = useContext(FilmContext);
  const select = enter(frame, BAR + 2 * BEAT, 8);
  return (
    <Group x={(STAGE.width - PV.width) / 2}>
      <Group clip={{ width: PV.width, height: PV.height, cornerRadius: 6 }}>
        <Group scale={PV.width / W}><FreezeFrame frame={PREVIEW_AT}><Film /></FreezeFrame></Group>
      </Group>
      {/* The selected layer: the preview's picture. */}
      <Rect x={PV.width * 0.47} y={PV.height * 0.12} width={PV.width * 0.47} height={PV.height * 0.76}
        stroke={C.blue} strokeWidth={3} opacity={select} />
      <Group x={PV.width * 0.47} y={PV.height * 0.88 + 12} opacity={enter(frame, 2 * BAR, 8)}>
        <Rect width={360} height={40} cornerRadius={8} fill={C.paper} />
        <Text x={14} y={20} anchorY={0.5} style={mono(16, C.ink, 700)}>{'→ v0.7.0/demos/ColorDemo.tsx:62'}</Text>
      </Group>
    </Group>
  );
}

function Timeline({ frame }: { frame: number }) {
  const width = STAGE.width;
  // Pinch on bar 2: the frames-per-pixel halves.
  const zoom = interpolate(frame, [BAR, BAR + 18], [1, 2.4], { easing: Easings.easeInOutCubic, extrapolateLeft: 'clamp', extrapolateRight: 'clamp' });
  const span = DURATION / zoom;
  // The playhead is this chapter's own frame of the film.
  const head = chapterAt(11) + frame;
  const start = Math.max(0, Math.min(DURATION - span, head - span / 2));
  const x = (f: number) => ((f - start) / span) * (width - 110) + 110;
  const tracks = [
    { name: 'video', clips: Array.from({ length: 13 }, (_, i) => [chapterAt(i), chapterAt(i + 1)] as const) },
  ];
  const pinch = frame >= BAR - 4 && frame < BAR + 24;
  const spread = 40 + 70 * progress(frame, BAR, 18, Easings.easeInOutCubic);
  return (
    <Group>
      <Rect width={width} height={250} cornerRadius={8} fill={C.panel} stroke={C.line} strokeWidth={2} />
      <Group clip={{ x: 110, width: width - 110, height: 250 }}>
        {tracks[0].clips.map(([a, b], i) => (
          <Rect key={i} x={x(a) + 1} y={44} width={Math.max(2, x(b) - x(a) - 2)} height={56} cornerRadius={5}
            fill={i % 2 ? '#2C4A6E' : '#35577F'} stroke={C.blue} strokeWidth={1} />
        ))}
        <Rect x={x(0)} y={118} width={x(DURATION) - x(0)} height={86} cornerRadius={5} fill="#1E3A2A" stroke={C.green} strokeWidth={1} />
        {Array.from({ length: 140 }, (_, i) => {
          const f = start + (i / 140) * span;
          const level = scoreAt(f).rms;
          const h = 4 + 70 * Math.min(1, level * 1.5);
          return <Rect key={i} x={110 + (i / 140) * (width - 110)} y={161 - h / 2} width={(width - 110) / 140 - 1.5} height={h} fill={C.green} opacity={0.8} />;
        })}
        <Rect x={x(head)} y={20} width={2} height={220} fill={C.orange} />
      </Group>
      <Text x={14} y={72} anchorY={0.5} style={mono(16, C.grey)}>video</Text>
      <Text x={14} y={161} anchorY={0.5} style={mono(16, C.grey)}>audio</Text>
      <Text x={124} y={20} anchorY={0.5} style={mono(14, C.dim)}>{`score-0.7.0.wav   ·   ${zoom.toFixed(1)}×`}</Text>
      {pinch ? (
        <Group x={width / 2} y={130} opacity={0.9}>
          <Circle x={-spread} anchorX={0.5} anchorY={0.5} radius={18} fill="#FFFFFF55" stroke={C.paper} strokeWidth={2} />
          <Circle x={spread} anchorX={0.5} anchorY={0.5} radius={18} fill="#FFFFFF55" stroke={C.paper} strokeWidth={2} />
        </Group>
      ) : null}
    </Group>
  );
}

function Problems({ frame }: { frame: number }) {
  const p = progress(frame, 3 * BAR, 10, Easings.easeOutCubic);
  if (p <= 0) return null;
  const lines = [
    "TypeError: Cannot read properties of undefined (reading 'bands')",
    '    at SpectrumDemo (v0.7.0/demos/SpectrumDemo.tsx:42:30)',
    '    at ChapterShell (components/ChapterShell.tsx:141:9)',
  ];
  return (
    <Group y={STAGE.height - 190 * p}>
      <Rect width={STAGE.width} height={190} cornerRadius={8} fill="#1A1215" stroke={C.red} strokeWidth={2} />
      <Text x={18} y={26} anchorY={0.5} style={mono(17, C.paper, 700)}>PROBLEMS  1</Text>
      <Group x={STAGE.width - 110} y={10}>
        <Rect width={92} height={32} cornerRadius={6} fill={C.raised} />
        <Text x={46} y={16} anchorX={0.5} anchorY={0.5} style={mono(15, C.paper)}>Copy</Text>
      </Group>
      {lines.map((line, i) => (
        <Text key={i} x={18} y={66 + i * 32} anchorY={0.5} style={mono(15, i === 0 ? C.red : C.grey)}>{line}</Text>
      ))}
    </Group>
  );
}

export function EditorDemo({ frame }: DemoProps) {
  return (
    <Group>
      <Group opacity={enter(frame, 0)}><Preview frame={frame} /></Group>
      <Group y={300} opacity={enter(frame, 6)}><Timeline frame={frame} /></Group>
      <Problems frame={frame} />
    </Group>
  );
}
