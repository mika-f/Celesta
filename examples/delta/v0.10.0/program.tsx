import type { ComponentType, ReactNode } from 'react';
import { Easings, Group, Rect, Sequence, Span, Text, interpolate, useCurrentFrame, useScene } from '@celesta.studio/react';
import type { CodeLanguage } from '@celesta-plugin/code';
import { CaptionTrack, LowerThird } from '@celesta.studio/components';
import { LayoutBounds } from '@celesta.studio/layout';
import { Backdrop } from '../components/Backdrop';
import { Delta } from '../components/Delta';
import { DiffPanel } from '../components/DiffPanel';
import { C, FONT, H, ease, solid } from '../components/theme';
import { Cable, MODULE, Module, STRIP, JACK_Y, jackX, number } from './module';
import type { Ports } from './module';
import { CABLE } from './theme';
import { BLOCKS, CHAPTER_LENGTH, FEATURES, SCENE_LIST, blockOf } from './timeline';

// One feature, patched in: what it is and the lines that use it on the
// left, its module on the right with the demo on the module's screen, the
// sentence on the strip under the screen lit word by word (CaptionTrack),
// and the feature's name and pull requests on a lower third. The cable
// plugs into the module's first input on the downbeat and its LED lights;
// near the end of the two bars a second cable goes into its output, on to
// the next module.

export interface DemoProps {
  /** Chapter frame: 0 on the downbeat the module is patched in. */
  frame: number;
  /** False where the demo runs small in a slot of the rack. */
  live: boolean;
}

export interface Chapter {
  /** What the feature is, in a few words, for the label over the headline. */
  key: string;
  title: string;
  /** One or two lines; may hold <Span>s. */
  headline: ReactNode;
  /** The sentence on the strip, lit word by word. */
  caption: string;
  file: string;
  language?: CodeLanguage;
  code: string;
  merged: readonly (readonly [number, string])[];
  /** Right-hand label of the module's header. */
  meta?: string;
  ports: Ports;
  Demo: ComponentType<DemoProps>;
}

const accent = { fill: C.orange };
/** A headline's highlighted words: a <Span> itself, as <Text> takes no other element. */
export const hi = (text: string) => <Span style={accent}>{text}</Span>;

const mono = (size: number, color: string, weight = 400) =>
  ({ fontFamily: FONT.mono, fontSize: size, fontWeight: weight, fill: solid(color) }) as const;

/** When the input cable is in and the LED lights, and when the output cable goes in. */
export const PLUG_IN = 8;
export const PLUG_OUT = CHAPTER_LENGTH - 22;

/** The module's power at a chapter frame: off until the cable is in. */
export const powerAt = (frame: number) => interpolate(frame, [PLUG_IN - 1, PLUG_IN + 5], [0, 1], { extrapolateLeft: 'clamp', extrapolateRight: 'clamp' });

/** A chapter's module, its demo at `frame`. */
export function ChapterModule({ chapter, index, frame, live, power, caption }: {
  chapter: Chapter; index: number; frame: number; live: boolean; power: number; caption?: ReactNode;
}) {
  const { Demo } = chapter;
  return (
    <Module index={index} title={chapter.title} meta={chapter.meta} ports={chapter.ports} power={power}
      cable={blockOf(index).cable} caption={caption}>
      <Demo frame={frame} live={live} />
    </Module>
  );
}

/** The sentence on the module's strip, a word at a time over the chapter. */
export function Caption({ text, width = STRIP.width, fontSize = 31, maxLines = 2 }:
  { text: string; width?: number; fontSize?: number; maxLines?: number }) {
  return (
    <Group x={STRIP.x} y={STRIP.y}>
      <LayoutBounds width={width} height={Math.ceil(fontSize * 1.45 * maxLines) + 16}>
        <CaptionTrack cues={[{ from: 10, durationInFrames: CHAPTER_LENGTH - 26, text }]} subtitles={false}
          position="top" align="left" margin={0} maxWidth={width} maxLines={maxLines}
          fontFamily="sans" fontSize={fontSize} lineHeight={1.45} color="paper" style={{ fontWeight: 500 }}
          background={null} padding={0}
          highlight={{ color: C.ink, background: C.orange, spokenColor: C.paper, upcomingColor: '#EFECE466', cornerRadius: 3 }} />
      </LayoutBounds>
    </Group>
  );
}

/**
 * The rack in miniature: sixteen slots, the core first and MORE last, the
 * modules patched so far lit in their block's color and this one filling
 * with the chapter's progress. It sits inside the chapter's Sequence, so it
 * reads the film's scene list on the composition's clock.
 */
function RackMap({ y }: { y: number }) {
  const scene = useScene(SCENE_LIST, { clock: 'root' });
  if (!scene) return null;
  const current = scene.index - 1;
  const cell = { width: 72, height: 40, pitch: 86 };
  return (
    <Group x={90} y={y}>
      <Text style={mono(17, C.dim)}>RACK</Text>
      <Text x={690} anchorX={1} style={mono(17, C.dim)}>{`${Math.min(current + 1, FEATURES)} / ${FEATURES} PATCHED`}</Text>
      {Array.from({ length: 16 }, (_, slot) => {
        const i = slot - 1;
        const col = slot % 8;
        const row = Math.floor(slot / 8);
        const x = col * cell.pitch;
        const top = 34 + row * (cell.height + 10);
        const color = slot === 0 ? C.paper : i >= FEATURES ? C.grey : CABLE[blockOf(i).cable];
        const done = slot === 0 || i < current;
        const on = i === current;
        return (
          <Group key={slot} x={x} y={top}>
            <Rect width={cell.width} height={cell.height} fill={done ? `${color}2A` : 'panel'} stroke={on || done ? color : 'line'} strokeWidth={on ? 3 : 1} />
            {on ? <Rect y={cell.height - 6} width={cell.width * scene.progress} height={6} fill={color} /> : null}
            <Text x={8} y={5} style={mono(15, on ? C.paper : done ? color : C.dim, on ? 700 : 400)}>
              {slot === 0 ? 'CORE' : i >= FEATURES ? 'MORE' : number(i)}
            </Text>
          </Group>
        );
      })}
      <Group y={34 + 2 * (cell.height + 10) + 4}>
        {BLOCKS.map((block, n) => (
          <Group key={block.name} x={n * 172}>
            <Rect y={4} width={14} height={14} fill={block.cable} />
            <Text x={24} style={mono(16, C.grey)}>{block.name}</Text>
          </Group>
        ))}
      </Group>
    </Group>
  );
}

/** A cable's swing after its plug goes in: a damped sway, in pixels. */
export const swayAt = (f: number, amount = 60) => (f <= 0 ? 0 : amount * Math.sin(f * 0.32) * Math.exp(-f / 22));

/** The plug's travel into its jack: out by 80 px, in by `at`, with an overshoot. */
const plugAt = (f: number, at: number) =>
  interpolate(f, [at - 8, at], [80, 0], { easing: Easings.easeOutBack, extrapolateLeft: 'clamp', extrapolateRight: 'clamp' });

/** The cables at the module's jacks, in frame coordinates. */
function Cables({ index, frame }: { index: number; frame: number }) {
  const inColor = CABLE[blockOf(index).cable];
  const next = index + 1 < FEATURES ? CABLE[blockOf(index + 1).cable] : C.paper;
  const y = MODULE.y + JACK_Y;
  const xIn = MODULE.x + jackX('in', 0);
  const xOut = MODULE.x + jackX('out', 0);
  const outIn = frame >= PLUG_OUT - 8;
  return (
    <Group>
      <Cable x={xIn} y={y} end={[xIn - 330, H + 80]} color={inColor} plug={plugAt(frame, PLUG_IN)} sway={swayAt(frame - PLUG_IN)} />
      {outIn ? <Cable x={xOut} y={y} end={[xOut + 260, H + 80]} color={next} plug={plugAt(frame, PLUG_OUT)} sway={swayAt(frame - PLUG_OUT, -50)} /> : null}
    </Group>
  );
}

/** One chapter, patched in. */
export function Program({ chapter, index, live = true }: { chapter: Chapter; index: number; live?: boolean }) {
  const frame = useCurrentFrame();
  const head = ease(frame, 4, 20);
  const code = ease(frame, 12, 16);
  const typed = Math.max(0, Math.floor((frame - 16) * 2.6));
  return (
    <Group>
      <Backdrop />
      <Group x={92} y={MODULE.y + 29}>
        <Delta x={12} y={0} size={24} />
        <Text x={38} anchorY={0.5} style={mono(22, C.orange, 700)}>{`Δ${number(index)}`}</Text>
        <Text x={112} anchorY={0.5} style={mono(22, C.grey)}>{chapter.key}</Text>
      </Group>
      <Text x={90} y={MODULE.y + 66 + 24 * (1 - head)} opacity={head} maxWidth={700}
        style={{ fontFamily: FONT.sans, fontSize: 62, fontWeight: 700, lineHeight: 74, fill: solid(C.paper) }}>
        {chapter.headline}
      </Text>
      <Group opacity={code} y={16 * (1 - code)}>
        <DiffPanel x={90} y={MODULE.y + 250} width={690} file={chapter.file} code={chapter.code} language={chapter.language} typed={typed} />
      </Group>
      <Group opacity={ease(frame, 20, 16)}>
        <RackMap y={MODULE.y + 500} />
      </Group>
      <Group x={MODULE.x} y={MODULE.y}>
        <ChapterModule chapter={chapter} index={index} frame={frame} live={live} power={powerAt(frame)}
          caption={<Caption text={chapter.caption} />} />
      </Group>
      <Cables index={index} frame={frame} />
      <Sequence from={6} durationInFrames={CHAPTER_LENGTH - 14}>
        <LowerThird title={chapter.title} subtitle={chapter.merged.map(([n, label]) => `#${n} ${label}`).join('  ·  ')}
          margin={70} maxWidth={720} fontFamily="display" color="paper" subtitleColor="grey"
          titleSize={112} minTitleSize={64} maxTitleLines={1} titleLineHeight={1.0}
          titleStyle={{ letterSpacing: { em: 0.01 } }}
          subtitleSize={20} minSubtitleSize={16} maxSubtitleLines={2} subtitleStyle={{ fontFamily: FONT.mono, fontWeight: 700 }}
          background="raised" accent={blockOf(index).cable} accentWidth={10} cornerRadius={4}
          padding={{ top: 18, right: 34, bottom: 20, left: 34 }} gap={6} />
      </Sequence>
    </Group>
  );
}
