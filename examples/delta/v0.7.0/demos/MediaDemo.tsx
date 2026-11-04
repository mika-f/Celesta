import { Group, Rect, Text } from '@celesta.studio/react';
import { Circle } from '@celesta.studio/shapes';
import type { DemoProps } from '../../components/ChapterShell';
import { STAGE, enter } from '../../components/ChapterShell';
import { Delta } from '../../components/Delta';
import { C, FPS, FONT, solid } from '../../components/theme';
import { scoreAt } from '../score';
import { BAR, chapterAt } from '../timeline';
import { Picture } from './Picture';

// Three files, drawn: a <Video> whose soundtrack plays (its waveform is the
// score's loudness at these frames) and loops every two bars; an animated
// image whose frame follows the clock, not the render order; and a frame
// exported with alpha, over a checkerboard.
const AT = chapterAt(9);
const W = STAGE.width;
const mono = (size: number, color: string, weight = 400) =>
  ({ fontFamily: FONT.mono, fontSize: size, fontWeight: weight, fill: solid(color) }) as const;

function VideoCard({ frame }: { frame: number }) {
  const loopLength = 2 * BAR;
  const local = ((frame % loopLength) + loopLength) % loopLength;
  const loops = Math.floor(Math.max(0, frame) / loopLength) + 1;
  const width = 520;
  const height = 190;
  return (
    <Group>
      <Group clip={{ width, height: height + 60, cornerRadius: 8 }}>
        <Picture width={width} height={height} t={local * 3} />
        <Rect y={height} width={width} height={60} fill={C.raised} />
        {Array.from({ length: 80 }, (_, i) => {
          const level = scoreAt(AT + frame - 40 + i).rms;
          const h = 4 + 44 * Math.min(1, level * 1.5);
          return <Rect key={i} x={i * (width / 80)} y={height + 30 - h / 2} width={width / 80 - 2} height={h} fill={i === 40 ? C.paper : C.orange} />;
        })}
      </Group>
      <Text x={width + 24} y={10} style={mono(20, C.paper, 700)}>{'<Video src="clip.mp4"'}</Text>
      <Text x={width + 24} y={42} style={mono(20, C.paper)}>{'  volume={0.6} loop />'}</Text>
      <Text x={width + 24} y={100} style={mono(18, C.grey)}>{`t ${(local / FPS).toFixed(2)} s`}</Text>
      <Group x={width + 24} y={132}>
        <Rect width={150} height={40} cornerRadius={20} fill={C.orange} />
        <Text x={75} y={20} anchorX={0.5} anchorY={0.5} style={mono(18, C.ink, 700)}>{`loop ×${loops}`}</Text>
      </Group>
      <Text x={width + 24} y={206} style={mono(18, C.green)}>muted={'{false}'}: sound on</Text>
    </Group>
  );
}

const SPIN_FRAMES = 12;
const SPIN_MS = 80;

function Spinner({ frame }: { frame: number }) {
  const ms = (Math.max(0, frame) / FPS) * 1000;
  const index = Math.floor(ms / SPIN_MS) % SPIN_FRAMES;
  return (
    <Group>
      <Rect width={190} height={190} cornerRadius={8} fill={C.raised} />
      <Group x={95} y={95}>
        {Array.from({ length: SPIN_FRAMES }, (_, i) => {
          const age = (index - i + SPIN_FRAMES) % SPIN_FRAMES;
          return (
            <Rect key={i} rotation={(i * 360) / SPIN_FRAMES} x={0} y={0} anchorX={0.5} anchorY={1.9}
              width={14} height={36} cornerRadius={7} fill={age === 0 ? C.orange : C.paper} opacity={Math.max(0.12, 1 - age / 8)} />
          );
        })}
      </Group>
      <Text x={214} y={10} style={mono(20, C.paper, 700)}>{'<Image src="spinner.png" />'}</Text>
      <Text x={214} y={46} style={mono(18, C.grey)}>APNG · 12 frames · 80 ms each</Text>
      <Text x={214} y={86} style={mono(22, C.orange, 700)}>{`frame ${String(index + 1).padStart(2, '0')} / ${SPIN_FRAMES}`}</Text>
      <Text x={214} y={122} style={mono(18, C.grey)}>{`at ${ms.toFixed(0)} ms, scrubbed or exported`}</Text>
      <Text x={214} y={152} style={mono(18, C.grey)}>GIF · APNG · WebP</Text>
    </Group>
  );
}

function Alpha({ frame }: { frame: number }) {
  const cell = 20;
  const width = 360;
  const height = 190;
  return (
    <Group>
      <Group clip={{ width, height, cornerRadius: 8 }}>
        {Array.from({ length: (width / cell) * (height / cell + 1) }, (_, i) => {
          const x = i % (width / cell);
          const y = Math.floor(i / (width / cell));
          return (x + y) % 2 ? <Rect key={i} x={x * cell} y={y * cell} width={cell} height={cell} fill="#2A2C33" /> : null;
        })}
        <Rect width={width} height={height} fill="#00000000" />
        <Circle x={110} y={95} anchorX={0.5} anchorY={0.5} radius={62 + 6 * Math.sin(frame / 6)} fill="#F26A2ECC" />
        <Delta x={110} y={98} size={70} fill={C.ink} />
        <Text x={190} y={95} anchorY={0.5} style={{ fontFamily: FONT.display, fontSize: 64, fill: solid(C.paper) }}>LIVE</Text>
      </Group>
      <Group x={width + 24}>
        <Text y={10} style={mono(20, C.paper, 700)}>overlay.mov</Text>
        <Text y={40} style={mono(18, C.grey)}>ProRes 4444 · yuva444p10le</Text>
        <Text y={92} style={mono(20, C.paper, 700)}>overlay.webm</Text>
        <Text y={122} style={mono(18, C.grey)}>VP9 · yuva420p</Text>
        <Text y={168} style={mono(18, C.green)}>alpha: true</Text>
      </Group>
    </Group>
  );
}

export function MediaDemo({ frame }: DemoProps) {
  return (
    <Group x={10} y={14}>
      <Group opacity={enter(frame, 0)}><VideoCard frame={frame} /></Group>
      <Group y={290} opacity={enter(frame, 10)}><Spinner frame={frame} /></Group>
      <Group y={510} opacity={enter(frame, 20)}><Alpha frame={frame} /></Group>
      <Rect x={-10} y={270} width={W} height={2} fill={C.line} />
      <Rect x={-10} y={490} width={W} height={2} fill={C.line} />
    </Group>
  );
}
