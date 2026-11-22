import { Composition, Rect, Scenes, Text, defineScenes } from '@celesta.studio/react';

// A rough cut: the film's scenes, their lengths and one sentence on what
// happens in each, agreed before every scene is built. A scene marked
// `status: 'rough'` (or one without a `Scene`) plays as a placeholder card
// with its name, `desc`, place and length; the editor's Chapters row greys
// it out and tags it Rough, and an export warns about it (`--final`
// refuses it). Build a scene, give it its `Scene`, and drop the status.

const W = 1920;
const H = 1080;

function Title() {
  return (
    <>
      <Rect width={W} height={H} fill="#0f2a3d" />
      <Text x={W / 2} y={H / 2} anchorX={0.5} anchorY={0.5} fontSize={120} color="#eaf4f2">
        Blueprint
      </Text>
    </>
  );
}

const SCENES = defineScenes([
  { id: 'title', name: 'Title', durationInFrames: 60, desc: 'The film\'s name on the blueprint ground.', Scene: Title },
  { id: 'card', name: 'The card', durationInFrames: 105, status: 'rough', desc: 'A card slides onto the blueprint and its corners snap to the grid.' },
  { id: 'pour', name: 'The pour', durationInFrames: 90, status: 'rough', desc: 'A ladle tips and fills the card with color.' },
  { id: 'logo', name: 'Logo', durationInFrames: 45, status: 'rough' },
]);

export default function Root() {
  return (
    <Composition width={W} height={H} fps={30} durationInFrames={SCENES.durationInFrames} chapters={SCENES}>
      <Scenes scenes={SCENES} />
    </Composition>
  );
}
