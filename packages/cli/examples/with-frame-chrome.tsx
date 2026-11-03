import { FrameChrome } from '@celesta.studio/components';
import { Composition, Font, Rect, Scenes, Text, Theme, defineScenes } from '@celesta.studio/react';

// <FrameChrome> draws the overlay a film keeps around the picture: corner
// brackets, a title, the timecode, the current chapter, and a progress bar
// split into one segment per scene. Blended with `difference`, it reads over
// the dark scene and the light one alike.

const colors = { paper: '#F4F1EA', ink: '#111111', accent: '#FF4D1F' };

function Card({ bg, fg, children }: { bg: string; fg: string; children: string }) {
  return <>
    <Rect width={960} height={540} fill={bg} />
    <Text x={480} y={270} anchorX={0.5} anchorY={0.5} fontFamily="display" fontSize={64} fontWeight={700} color={fg}>
      {children}
    </Text>
  </>;
}

const SCENES = defineScenes([
  { id: 'intro', name: 'COLD OPEN', durationInFrames: 45, Scene: () => <Card bg={colors.ink} fg="paper">Intro</Card> },
  { id: 'body', name: 'MANIFESTO', durationInFrames: 75, Scene: () => <Card bg={colors.paper} fg="ink">Body</Card> },
]);

export default function WithFrameChrome() {
  return (
    <Composition width={960} height={540} fps={30} durationInFrames={SCENES.durationInFrames}>
      <Font google="Inter" weights={[700]} />
      <Font google="JetBrains Mono" weights={[400, 700]} />
      <Theme fonts={{ display: 'Inter', mono: 'JetBrains Mono' }} colors={colors}>
        <Scenes scenes={SCENES} />
        <FrameChrome margin={32} corners color="paper" blendMode="difference" fontFamily="mono" fontSize={14}
          title="CELESTA" subtitle="/ FRAME CHROME" timecode timecodeHours={false}
          chapters scenes={SCENES} formatChapter={(scene) => `0${scene.index + 1}  ${scene.name}`}
          caption="960×540 · 30 FPS" progress />
      </Theme>
    </Composition>
  );
}
