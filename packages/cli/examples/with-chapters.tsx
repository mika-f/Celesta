import { Composition, Rect, Scenes, Text, defineScenes, listChapters, useScene } from '@celesta.studio/react';

// Chapters for the editor preview. The scenes are declared once with
// defineScenes: <Scenes> plays them, and `chapters` on the <Composition>
// lists them in the preview, where the timeline's Chapters row and the
// React Preview tab seek to each one. The parts of the middle scene, laid
// out on its own frames, join the list moved to where that scene starts.

const W = 1280;
const H = 720;

function Card({ color, title }: { color: string; title: string }) {
  return (
    <>
      <Rect width={W} height={H} fill={{ type: 'solid', color }} />
      <Text x={W / 2} y={H / 2} anchorX={0.5} anchorY={0.5}
        style={{ fontFamily: 'sans-serif', fontSize: 96, fontWeight: 700, fill: { type: 'solid', color: '#ffffff' } }}>
        {title}
      </Text>
    </>
  );
}

// The middle scene's parts, in its own frames.
const PARTS = defineScenes([
  { id: 'demo-setup', name: 'Demo: setup', durationInFrames: 45 },
  { id: 'demo-result', name: 'Demo: result', durationInFrames: 45 },
]);

const SCENES = defineScenes([
  { id: 'intro', name: 'Intro', durationInFrames: 60, Scene: () => <Card color="#2b59c3" title="Intro" /> },
  { id: 'demo', name: 'Demo', durationInFrames: PARTS.durationInFrames, Scene: () => <Card color="#d9534f" title="Demo" /> },
  { id: 'outro', name: 'Outro', durationInFrames: 60, Scene: () => <Card color="#2f9e6e" title="Outro" /> },
]);

const CHAPTERS = [
  ...listChapters(SCENES),
  ...listChapters(PARTS, { from: SCENES.scenes[1].from }),
];

/** The current scene's name and progress, read beside the <Scenes>. */
function Hud() {
  const scene = useScene(SCENES);
  if (!scene) return null;
  return (
    <>
      <Rect y={H - 8} width={W * scene.progress} height={8} fill={{ type: 'solid', color: '#ffffff' }} />
      <Text x={32} y={32} style={{ fontFamily: 'sans-serif', fontSize: 28, fill: { type: 'solid', color: '#ffffff' } }}>
        {`${scene.index + 1} / ${scene.scenes.length}  ${scene.name}`}
      </Text>
    </>
  );
}

export default function Root() {
  return (
    <Composition width={W} height={H} fps={30} durationInFrames={SCENES.durationInFrames} chapters={CHAPTERS}>
      <Scenes scenes={SCENES} />
      <Hud />
    </Composition>
  );
}
