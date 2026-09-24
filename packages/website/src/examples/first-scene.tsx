import {
  Composition, Rect, Text, useCurrentFrame,
} from '@celesta/react';

function Title() {
  const frame = useCurrentFrame();

  return (
    <Text
      x={960} y={540}
      anchorX={0.5} anchorY={0.5}
      opacity={Math.min(frame / 30, 1)}
      style={{
        fontFamily: 'sans-serif',
        fontSize: 96,
        fill: { type: 'solid', color: '#57456c' },
        align: 'center',
      }}
    >
      Hello, Celesta.
    </Text>
  );
}

export default function Root() {
  return (
    <Composition
      width={1920} height={1080}
      fps={30} durationInFrames={150}
    >
      <Rect width={1920} height={1080} fill="#e4daf0" />
      <Title />
    </Composition>
  );
}
