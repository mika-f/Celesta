import type { ComponentType, ReactNode } from 'react';
import { Group, Rect, Sequence, Span, Text, useCurrentFrame, useScene } from '@celesta/react';
import type { CodeLanguage } from '@celesta/code';
import { CaptionTrack, LowerThird } from '@celesta/components';
import { LayoutBounds } from '@celesta/layout';
import { Backdrop } from '../components/Backdrop';
import { Delta } from '../components/Delta';
import { DiffPanel } from '../components/DiffPanel';
import { C, FONT, ease, solid } from '../components/theme';
import { CHAPTER_LENGTH, SCENE_LIST } from './timeline';

// One feature as the program goes out on air: what it is and the lines
// that use it on the left, its demo on the program monitor, the sentence
// under the monitor lit word by word (CaptionTrack, #451 #499), and the
// feature's name and pull requests on a lower third (LowerThird, #442).

/** The program monitor. Every demo draws into STAGE, at the monitor's size. */
export const MONITOR = { x: 836, y: 176, width: 1000, height: 563 } as const;
export const STAGE = { width: MONITOR.width, height: MONITOR.height } as const;

export interface DemoProps {
  /** Chapter frame: 0 on the downbeat the chapter goes on air. */
  frame: number;
  /** False where the demo is a source on the multiviewer: no sound, nothing that speaks. */
  live: boolean;
}

export interface Chapter {
  /** What the feature is, in a few words, for the label over the headline. */
  key: string;
  title: string;
  /** One or two lines; may hold <Span>s. */
  headline: ReactNode;
  /** The sentence under the monitor, lit word by word. */
  caption: string;
  file: string;
  language?: CodeLanguage;
  code: string;
  merged: readonly (readonly [number, string])[];
  /** Right-hand label of the monitor's header. */
  meta?: string;
  Demo: ComponentType<DemoProps>;
}

const accent = { fill: C.orange };
/** A headline's highlighted words: a <Span> itself, as <Text> takes no other element. */
export const hi = (text: string) => <Span style={accent}>{text}</Span>;

const mono = (size: number, color: string, weight = 400) =>
  ({ fontFamily: FONT.mono, fontSize: size, fontWeight: weight, fill: solid(color) }) as const;

export const number = (index: number) => String(index + 1).padStart(2, '0');

/** The monitor: a header with the tally and the source's name, the demo inside a bezel, at `scale` of STAGE. */
export function Monitor({ index, chapter, frame, live, tally = 'tally', x = MONITOR.x, y = MONITOR.y, scale = 1, meta = true }: {
  index: number; chapter: Chapter; frame: number; live: boolean; tally?: string;
  x?: number; y?: number; scale?: number;
  /** Draws the chapter's meta at the header's right end. */
  meta?: boolean;
}) {
  const { Demo } = chapter;
  const width = STAGE.width * scale;
  const height = STAGE.height * scale;
  return (
    <Group x={x} y={y}>
      <Rect x={0} y={-44} width={18} height={18} fill={tally} />
      <Text x={30} y={-35} anchorY={0.5} style={mono(20, C.paper, 700)}>PGM</Text>
      <Text x={88} y={-35} anchorY={0.5} style={mono(20, C.grey)}>{`IN ${number(index)}  ${chapter.title}`}</Text>
      {meta && chapter.meta ? <Text x={width} y={-35} anchorX={1} anchorY={0.5} style={mono(20, C.dim)}>{chapter.meta}</Text> : null}
      <Rect x={-6} y={-6} width={width + 12} height={height + 12} stroke="line" strokeWidth={2} />
      <Group clip={{ width, height }}>
        <Group scale={scale}>
          <Rect width={STAGE.width} height={STAGE.height} fill="panel" />
          <Demo frame={frame} live={live} />
        </Group>
      </Group>
    </Group>
  );
}

/** The caption under the monitor: the chapter's sentence, a word at a time over the chapter. */
export function Caption({ text, x = MONITOR.x, y = MONITOR.y + MONITOR.height + 22, width = MONITOR.width, fontSize = 33, maxLines = 2 }:
  { text: string; x?: number; y?: number; width?: number; fontSize?: number; maxLines?: number }) {
  return (
    <Group x={x} y={y}>
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
 * The rundown: the scene before, this one with its progress, and the next.
 * It sits inside the chapter's Sequence, so it reads the film's scene list
 * on the composition's clock (useScene with clock: 'root', #472).
 */
function Rundown({ y }: { y: number }) {
  const scene = useScene(SCENE_LIST, { clock: 'root' });
  if (!scene) return null;
  const rows = [scene.index - 1, scene.index, scene.index + 1]
    .filter((i) => i >= 0 && i < SCENE_LIST.scenes.length)
    .map((i) => SCENE_LIST.scenes[i]);
  return (
    <Group x={90} y={y}>
      <Text style={mono(17, C.dim)}>RUNDOWN</Text>
      {rows.map((row) => {
        const on = row.id === scene.id;
        return (
          <Group key={row.id} y={34 + (row.index - rows[0].index) * 38}>
            <Text style={mono(20, on ? C.orange : C.dim, on ? 700 : 400)}>{String(row.from).padStart(4, '0')}</Text>
            <Text x={76} style={mono(20, on ? C.paper : C.grey, on ? 700 : 400)}>{row.name}</Text>
            {on ? (
              <Group x={400} y={9}>
                <Rect width={290} height={4} fill="line" />
                <Rect width={290 * scene.progress} height={4} fill="orange" />
              </Group>
            ) : <Text x={690} anchorX={1} style={mono(17, C.dim)}>{row.index < scene.index ? 'AIRED' : 'NEXT'}</Text>}
          </Group>
        );
      })}
    </Group>
  );
}

/** One chapter, as the program shows it. */
export function Program({ chapter, index, live = true }: { chapter: Chapter; index: number; live?: boolean }) {
  const frame = useCurrentFrame();
  const head = ease(frame, 4, 20);
  const code = ease(frame, 12, 16);
  const typed = Math.max(0, Math.floor((frame - 16) * 2.6));
  return (
    <Group>
      <Backdrop />
      <Group x={92} y={MONITOR.y - 35}>
        <Delta x={12} y={0} size={24} />
        <Text x={38} anchorY={0.5} style={mono(22, C.orange, 700)}>{`Δ${number(index)}`}</Text>
        <Text x={112} anchorY={0.5} style={mono(22, C.grey)}>{chapter.key}</Text>
      </Group>
      <Text x={90} y={MONITOR.y + 2 + 24 * (1 - head)} opacity={head} maxWidth={700}
        style={{ fontFamily: FONT.sans, fontSize: 62, fontWeight: 700, lineHeight: 74, fill: solid(C.paper) }}>
        {chapter.headline}
      </Text>
      <Group opacity={code} y={16 * (1 - code)}>
        <DiffPanel x={90} y={MONITOR.y + 186} width={690} file={chapter.file} code={chapter.code} language={chapter.language} typed={typed} />
      </Group>
      <Group opacity={ease(frame, 20, 16)}>
        <Rundown y={MONITOR.y + 432} />
      </Group>
      <Monitor index={index} chapter={chapter} frame={frame} live={live} />
      <Caption text={chapter.caption} />
      <Sequence from={6} durationInFrames={CHAPTER_LENGTH - 14}>
        <LowerThird title={chapter.title} subtitle={chapter.merged.map(([n, label]) => `#${n} ${label}`).join('  ·  ')}
          margin={70} maxWidth={720} fontFamily="display" color="paper" subtitleColor="grey"
          titleSize={128} minTitleSize={72} maxTitleLines={1} titleLineHeight={1.0}
          titleStyle={{ letterSpacing: { em: 0.01 } }}
          subtitleSize={20} minSubtitleSize={16} maxSubtitleLines={2} subtitleStyle={{ fontFamily: FONT.mono, fontWeight: 700 }}
          background="raised" accent="orange" accentWidth={10} cornerRadius={4}
          padding={{ top: 20, right: 34, bottom: 22, left: 34 }} gap={6} />
      </Sequence>
    </Group>
  );
}
