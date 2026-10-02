// Two characters talk through a short script whose timing comes entirely
// from their voice recordings: planDialogue() measures each line, adds a
// short gap after it and a longer pause before a new scene, and
// <DialogueSeries> plays the lines. The timeline at the bottom draws the
// plan itself — lines, gaps, and the scene lead-in — with a playhead.
//
// Assets: examples/assets/dialogue-demo (original characters; voices made
// with Open JTalk and the MMDAgent "Mei"/"Takumi" HTS voices, CC BY 3.0 —
// see its README.md).
import * as React from 'react';

import {
  Assets,
  Character,
  CharacterView,
  Composition,
  DialogueSeries,
  Easings,
  Group,
  Rect,
  Sequence,
  Text,
  interpolate,
  loadLipSync,
  planDialogue,
  useCurrentFrame,
} from '@celesta/react';
import type {
  AssetReference,
  CharacterViewReference,
  DialogueLine,
  DialoguePlan,
  LipSyncTrack,
  PlannedDialogueLine,
} from '@celesta/react';

import script from '../../../examples/assets/dialogue-demo/script.json';

const WIDTH = 1280;
const HEIGHT = 720;
const FPS = 30;
const DEMO = '../../../examples/assets/dialogue-demo';

type Speaker = 'shizuku' | 'komugi';
type Line = DialogueLine & { speaker: Speaker };

const CAST: Record<Speaker, { name: string; color: string; x: number }> = {
  shizuku: { name: 'しずく', color: '#3f8fd6', x: 190 },
  komugi: { name: 'こむぎ', color: '#d97a2b', x: 730 },
};

const SCENES: Record<string, { title: string; background: [string, string] }> = {
  intro: { title: '1. はじめまして', background: ['#eaf4ff', '#cfe3f7'] },
  how: { title: '2. しくみ', background: ['#fff4e2', '#f6dfbd'] },
};

const characters = { shizuku: React.createRef<AssetReference>(), komugi: React.createRef<AssetReference>() };
const views = { shizuku: React.createRef<CharacterViewReference>(), komugi: React.createRef<CharacterViewReference>() };

// Filled once by prepare(): every line's timing, measured from its voice.
let plan: DialoguePlan<Line>;

export async function prepare(): Promise<void> {
  const lines = await Promise.all(
    script.map(async (line): Promise<Line> => {
      const audio = `${DEMO}/voices/${line.id}.wav`;
      const lipSync: LipSyncTrack = await loadLipSync({ src: audio, text: line.reading });
      return {
        id: line.id,
        scene: line.scene,
        speaker: line.speaker as Speaker,
        text: line.text,
        audio,
        expression: line.expression,
        lipSync,
      };
    }),
  );
  // A 0.3 s gap after each line, and a 1.5 s pause before a new scene.
  plan = await planDialogue(lines, { fps: FPS, gap: 0.3, sceneLeadIn: 1.5 });
}

function activeLine(frame: number): PlannedDialogueLine<Line> | undefined {
  return plan.lines.find((line) => frame >= line.from && frame < line.from + line.durationInFrames);
}

function Background({ scene }: { scene: string }) {
  const [top, bottom] = SCENES[scene].background;
  return (
    <Rect width={WIDTH} height={HEIGHT}
      fill={{ type: 'linear', start: { x: 0, y: 0 }, end: { x: 0, y: HEIGHT }, stops: [{ offset: 0, color: top }, { offset: 1, color: bottom }] }} />
  );
}

/** The scene title, shown during the scene's opening pause. Inside the scene's <Sequence>. */
function SceneTitle({ scene }: { scene: string }) {
  const frame = useCurrentFrame();
  const opacity = interpolate(frame, [0, 10, 35, 45], [0, 1, 1, 0], { extrapolateLeft: 'clamp', extrapolateRight: 'clamp' });
  const y = interpolate(frame, [0, 12], [20, 0], { extrapolateRight: 'clamp', easing: Easings.easeOutCubic });
  return (
    <Group opacity={opacity} y={y}>
      <Rect x={WIDTH / 2} y={70} anchorX={0.5} anchorY={0.5} width={360} height={64} cornerRadius={32} fill="#2b2340d0" />
      <Text x={WIDTH / 2} y={70} anchorX={0.5} anchorY={0.5}
        style={{ fontSize: 34, fontWeight: 700, fill: { type: 'solid', color: '#ffffff' } }}>
        {SCENES[scene].title}
      </Text>
    </Group>
  );
}

/** A character's portrait and name plate; the one speaking hops and stays bright. */
function Member({ speaker }: { speaker: Speaker }) {
  const frame = useCurrentFrame();
  const line = activeLine(frame);
  const speaking = line?.line.speaker === speaker;
  const since = speaking ? frame - line!.from : 99;
  const hop = interpolate(since, [0, 5, 12], [0, -14, 0], { extrapolateRight: 'clamp', easing: Easings.easeOutQuad });
  const { name, color, x } = CAST[speaker];
  return (
    <Group x={x} y={90 + hop} opacity={speaking || !line ? 1 : 0.6}>
      <CharacterView ref={views[speaker]} character={characters[speaker]} mouth="closed" />
      <Rect x={180} y={410} anchorX={0.5} anchorY={0.5} width={150} height={44} cornerRadius={22} fill={color} />
      <Text x={180} y={410} anchorX={0.5} anchorY={0.5} style={{ fontSize: 24, fontWeight: 700, fill: { type: 'solid', color: '#ffffff' } }}>
        {name}
      </Text>
    </Group>
  );
}

const TIMELINE = { x: 60, y: 640, width: WIDTH - 120, height: 26 };

/** The plan drawn to scale: a block per line, its gap, and the scene lead-in. */
function Timeline() {
  const frame = useCurrentFrame();
  const scale = TIMELINE.width / plan.durationInFrames;
  const at = (f: number) => TIMELINE.x + f * scale;
  return (
    <Group>
      <Rect x={TIMELINE.x - 10} y={TIMELINE.y - 34} width={TIMELINE.width + 20} height={TIMELINE.height + 56} cornerRadius={12} fill="#ffffffd8" />
      <Text x={TIMELINE.x} y={TIMELINE.y - 18} anchorY={0.5} style={{ fontSize: 15, fill: { type: 'solid', color: '#4a4560' } }}>
        {'planDialogue() のタイムライン　■ 台詞（音声の長さ）　□ 間　▨ 場面の前置き'}
      </Text>
      <Rect x={TIMELINE.x} y={TIMELINE.y} width={TIMELINE.width} height={TIMELINE.height} cornerRadius={4} fill="#e6e3ee" />
      {plan.lines.map((line) => (
        <React.Fragment key={line.id}>
          {line.leadInFrames > 0 ? (
            <Rect x={at(line.from - line.leadInFrames)} y={TIMELINE.y} width={line.leadInFrames * scale} height={TIMELINE.height} fill="#b9b2cc" />
          ) : null}
          <Rect x={at(line.from)} y={TIMELINE.y} width={line.durationInFrames * scale} height={TIMELINE.height}
            cornerRadius={4} fill={CAST[line.line.speaker].color} />
          <Rect x={at(line.from + line.durationInFrames)} y={TIMELINE.y + 4} width={line.gapInFrames * scale} height={TIMELINE.height - 8}
            fill="#ffffff" />
        </React.Fragment>
      ))}
      <Rect x={at(frame)} y={TIMELINE.y - 6} anchorX={0.5} width={3} height={TIMELINE.height + 12} fill="#2b2340" />
      <Text x={TIMELINE.x + TIMELINE.width} y={TIMELINE.y + TIMELINE.height + 12} anchorX={1} anchorY={0.5}
        style={{ fontSize: 13, fill: { type: 'solid', color: '#4a4560' } }}>
        {`${(frame / FPS).toFixed(1)} / ${(plan.durationInFrames / FPS).toFixed(1)} 秒`}
      </Text>
    </Group>
  );
}

function subtitle(color: string) {
  return {
    x: WIDTH / 2,
    y: 555,
    anchorX: 0.5,
    anchorY: 0.5,
    maxWidth: 1100,
    style: {
      fontSize: 38,
      fontWeight: 700,
      align: 'center' as const,
      fill: { type: 'solid' as const, color: '#ffffff' },
      stroke: { paint: { type: 'solid' as const, color }, width: 5 },
    },
  };
}

export default function Root() {
  return (
    <Composition width={WIDTH} height={HEIGHT} fps={FPS} durationInFrames={plan.durationInFrames}>
      <Assets>
        <Character
          ref={characters.shizuku}
          name="しずく"
          portrait={{
            defaultExpression: 'normal',
            expressions: { normal: `${DEMO}/portraits/shizuku-normal.png`, smile: `${DEMO}/portraits/shizuku-smile.png` },
            lipSync: mouths(),
          }}
          subtitle={subtitle(CAST.shizuku.color)}
        />
        <Character
          ref={characters.komugi}
          name="こむぎ"
          portrait={{
            defaultExpression: 'normal',
            expressions: { normal: `${DEMO}/portraits/komugi-normal.png`, smile: `${DEMO}/portraits/komugi-smile.png` },
            lipSync: mouths(),
          }}
          subtitle={subtitle(CAST.komugi.color)}
        />
      </Assets>
      {/* Each scene's backdrop and title follow the plan, so they cut with the lines. */}
      {plan.scenes.map((scene) => (
        <Sequence key={scene.id} from={scene.from} durationInFrames={scene.durationInFrames}>
          <Background scene={scene.id} />
          <SceneTitle scene={scene.id} />
        </Sequence>
      ))}
      <Member speaker="shizuku" />
      <Member speaker="komugi" />
      <DialogueSeries plan={plan} views={views} />
      <Timeline />
    </Composition>
  );
}

function mouths() {
  const mouth = (shape: string) => `${DEMO}/portraits/mouth-${shape}.png`;
  return { a: mouth('a'), i: mouth('i'), u: mouth('u'), e: mouth('e'), o: mouth('o'), closed: mouth('closed') };
}
