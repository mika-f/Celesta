import { Composition, Rect, Scenes, Text, defineScenes, useCurrentFrame } from '@celesta.studio/react';

// A scene list whose middle scene throws while it renders. In the editor
// preview only that scene draws its error card (the scene's name, id, and
// the error) and the scenes around it still play; the error is listed in
// Problems. An export, inspect.mjs, and the MCP tools fail on its frames
// instead of writing the card into a video.

const W = 1280;
const H = 720;

function Card({ color, title }: { color: string; title: string }) {
  const frame = useCurrentFrame();
  return (
    <>
      <Rect width={W} height={H} fill={{ type: 'solid', color }} />
      <Text x={W / 2} y={H / 2} anchorX={0.5} anchorY={0.5}
        style={{ fontFamily: 'sans-serif', fontSize: 96, fontWeight: 700, fill: { type: 'solid', color: '#ffffff' } }}>
        {`${title} ${frame}`}
      </Text>
    </>
  );
}

interface Stat {
  label: string;
  value: number;
}

// Data the scene expects but does not get: `stats[3]` is undefined.
const STATS: Stat[] = [
  { label: 'Views', value: 1200 },
  { label: 'Likes', value: 340 },
  { label: 'Shares', value: 56 },
];

function Broken() {
  const frame = useCurrentFrame();
  const stat = STATS[Math.floor(frame / 15)];
  return <Card color="#d9534f" title={`${stat.label}: ${stat.value}`} />;
}

const SCENES = defineScenes([
  { id: 'intro', name: 'Intro', durationInFrames: 60, Scene: () => <Card color="#2b59c3" title="Intro" /> },
  // Frames 0 to 44 of the scene draw; frame 45 on reads `STATS[3]`.
  { id: 'stats', name: 'Stats', durationInFrames: 90, Scene: Broken },
  { id: 'outro', name: 'Outro', durationInFrames: 60, Scene: () => <Card color="#2f9e6e" title="Outro" /> },
]);

export default function Root() {
  return (
    <Composition width={W} height={H} fps={30} durationInFrames={SCENES.durationInFrames}>
      <Scenes scenes={SCENES} />
    </Composition>
  );
}
