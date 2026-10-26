import { Composition, Group, Rect, Scenes, Sequence, Text, defineScenes, interpolate, useCurrentFrame } from '@celesta/react';

// Three scenes in a scene list written out as numbers, for trying timing on
// the editor's timeline: drag a scene's end on the Chapters row, drag the
// band of an element that has a <Sequence> of its own, and give an element
// one with "Set timing". A price tag in Intro and Main and a mark in every
// scene try "Edits change" too. README.md lists what to try.

const WIDTH = 1280;
const HEIGHT = 720;
const FPS = 30;
const INK = '#14161C';
const NIGHT = '#1E2A3A';
const PAPER = '#F2EEE6';
const ACCENT = '#E4572E';
// Where the circle in Main lands, in the scene's frames.
const LANDING = 60;

const type = (fontSize: number, color = PAPER) => ({ fontSize, fill: { type: 'solid', color } }) as const;

/** A tag Intro and Main both draw: its <Rect> is one element with two instances. */
function Tag({ label }: { label: string }) {
  return (
    <Group x={80} y={560}>
      <Rect width={320} height={72} cornerRadius={12} fill={ACCENT} />
      <Text x={24} y={20} style={type(32)}>{label}</Text>
    </Group>
  );
}

/** A price tag Intro and Main both draw, for trying "Edits change": its <Rect>'s width is written here once. */
interface PriceTagProps {
  label: string;
}

function PriceTag({ label }: PriceTagProps) {
  return (
    <Group x={880} y={560}>
      <Rect width={200} height={72} cornerRadius={12} fill={PAPER} />
      <Text x={20} y={20} style={type(32, INK)}>{label}</Text>
    </Group>
  );
}

/** A mark every scene draws: under "Shared value", its fill changes in every scene. */
function Mark() {
  return <Rect x={WIDTH - 120} y={40} width={80} height={24} cornerRadius={6} fill={ACCENT} />;
}

/** A title that fades in over the first 20 frames it reads. */
function Title() {
  const frame = useCurrentFrame();
  return (
    <Text x={80} y={120} opacity={interpolate(frame, [0, 20], [0, 1], { extrapolateRight: 'clamp' })} style={type(72)}>
      Scene timing
    </Text>
  );
}

function Intro() {
  return (
    <Group>
      {/* No timing of its own: "Set timing" wraps it. */}
      <Rect width={WIDTH} height={HEIGHT} fill={INK} />
      {/* The <Text> is Title's root, so it can't be wrapped; the <Group> can, and Title's fade then starts with it. */}
      <Group>
        <Title />
      </Group>
      {/* A window of its own: its band's edges and body drag. */}
      <Sequence from={30} durationInFrames={40}>
        <Text x={80} y={240} style={type(40, ACCENT)}>A Sequence of its own</Text>
      </Sequence>
      <Tag label="Intro" />
      <PriceTag label="Intro" />
      <Mark />
    </Group>
  );
}

function Main() {
  return (
    <Group>
      <Rect width={WIDTH} height={HEIGHT} fill={NIGHT} />
      {/* `from` is an expression: the start edge and the body are locked, the end drags. */}
      <Sequence from={LANDING - 30} durationInFrames={60}>
        <Rect x={WIDTH - 400} y={200} width={240} height={240} cornerRadius={120} fill={PAPER} />
      </Sequence>
      <Tag label="Main" />
      <PriceTag label="Sale" />
      <Mark />
    </Group>
  );
}

function Outro() {
  return (
    <Group>
      <Rect width={WIDTH} height={HEIGHT} fill={INK} />
      <Text x={WIDTH / 2} y={HEIGHT / 2} anchorX={0.5} anchorY={0.5} style={type(64)}>The end</Text>
      <Mark />
    </Group>
  );
}

// Each length is a number written in the entry, so the editor can drag it.
const SCENES = defineScenes([
  { id: 'intro', name: 'Intro', durationInFrames: 90, Scene: Intro },
  { id: 'main', name: 'Main', durationInFrames: 120, Scene: Main },
  { id: 'outro', name: 'Outro', durationInFrames: 90, Scene: Outro },
]);

export default function SceneTiming() {
  // The composition is as long as the scenes, so a scene drag changes both.
  return (
    <Composition width={WIDTH} height={HEIGHT} fps={FPS} durationInFrames={SCENES.durationInFrames} chapters={SCENES}>
      <Scenes scenes={SCENES} />
    </Composition>
  );
}
