import { Group, Rect, Text } from '@celesta.studio/react';
import { Lottie } from '@celesta.studio/lottie';
import type { DemoProps } from '../../components/ChapterShell';
import { CUT, STAGE, enter } from '../../components/ChapterShell';
import { C, FONT, FPS, solid } from '../../components/theme';
import { BEAT } from '../timeline';
import loaderCat from '../assets/loader-cat.json';

// "Loader cat" by Diane Picchiottino (LottieFiles, Lottie Simple License;
// see ../assets/README.md), a 280×200 file of 32 frames at 25 fps, drawn by
// <Lottie> at this film's 30 fps. Above, as it is, looping; below, the same
// file twice as fast, backwards, and held. The ruler counts the file's own
// frames: <Lottie> starts at its Sequence's frame 0, the chapter's first
// frame, CUT frames before the downbeat.
const mono = (size: number, color: string, weight = 400) =>
  ({ fontFamily: FONT.mono, fontSize: size, fontWeight: weight, fill: solid(color) }) as const;
const FILE = { fps: loaderCat.fr, frames: loaderCat.op - loaderCat.ip, width: loaderCat.w, height: loaderCat.h };
const HERO = { x: 40, y: 0, width: 780, height: 360 };
const CHIP = { width: 252, height: 220, gap: 12 };

/** The file frame <Lottie> shows on the chapter's frame `frame`, at `rate`. */
function fileFrame(frame: number, rate = 1) {
  const t = ((frame + CUT) / FPS) * FILE.fps * rate;
  return ((t % FILE.frames) + FILE.frames) % FILE.frames;
}

function Ruler({ frame, width }: { frame: number; width: number }) {
  const at = fileFrame(frame);
  const step = width / FILE.frames;
  return (
    <Group>
      {Array.from({ length: FILE.frames }, (_, i) => (
        <Rect key={i} x={i * step} y={i % 8 === 0 ? 0 : 8} width={2} height={i % 8 === 0 ? 22 : 14}
          fill={Math.floor(at) === i ? C.orange : C.dim} />
      ))}
      <Rect x={at * step - 1} y={-6} width={4} height={34} fill={C.orange} />
      <Text y={46} style={mono(18, C.grey)}>{`file frame ${at.toFixed(1).padStart(4, '0')} / ${FILE.frames}  ·  ${FILE.fps} fps  →  ${FPS} fps`}</Text>
    </Group>
  );
}

function Chip({ i, label, frame, children }: { i: number; label: string; frame: number; children: React.ReactNode }) {
  const p = enter(frame, 2 * BEAT + i * 4, 14);
  return (
    <Group x={20 + i * (CHIP.width + CHIP.gap)} y={480} opacity={p}>
      <Rect width={CHIP.width} height={CHIP.height} cornerRadius={12} fill={C.paper} />
      <Group x={CHIP.width / 2} y={CHIP.height / 2 - 12}>{children}</Group>
      <Text x={CHIP.width / 2} y={CHIP.height - 22} anchorX={0.5} anchorY={0.5} style={mono(16, C.ink, 700)}>{label}</Text>
    </Group>
  );
}

export function LottieDemo({ frame }: DemoProps) {
  const cat = { data: loaderCat, anchorX: 0.5, anchorY: 0.5 } as const;
  return (
    <Group>
      <Group opacity={enter(frame, 0)}>
        <Rect x={HERO.x} y={HERO.y} width={HERO.width} height={HERO.height} cornerRadius={14} fill={C.orange} />
        <Lottie {...cat} x={HERO.x + HERO.width / 2} y={HERO.y + HERO.height / 2 + 10} width={FILE.width * 1.6} />
        <Text x={HERO.x + 22} y={HERO.y + 30} anchorY={0.5} style={mono(17, C.ink, 700)}>{'<Lottie data={loaderCat} />'}</Text>
        <Text x={HERO.x + HERO.width - 22} y={HERO.y + 30} anchorX={1} anchorY={0.5} style={mono(15, '#0C0D0FAA')}>
          Loader cat · Diane Picchiottino · LottieFiles
        </Text>
      </Group>
      <Group x={HERO.x} y={392} opacity={enter(frame, BEAT)}>
        <Ruler frame={frame} width={HERO.width} />
      </Group>
      <Chip i={0} label="playbackRate={2}" frame={frame}>
        <Lottie {...cat} width={FILE.width * 0.72} playbackRate={2} />
      </Chip>
      <Chip i={1} label="playbackRate={-1}" frame={frame}>
        <Lottie {...cat} width={FILE.width * 0.72} playbackRate={-1} />
      </Chip>
      <Chip i={2} label="loop={false}" frame={frame}>
        <Lottie {...cat} width={FILE.width * 0.72} endFrame={12} loop={false} />
      </Chip>
      <Text x={STAGE.width / 2} y={STAGE.height - 18} anchorX={0.5} anchorY={0.5} opacity={enter(frame, 4 * BEAT)}
        style={mono(16, C.dim)}>Lottie Simple License</Text>
    </Group>
  );
}
