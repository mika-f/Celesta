import { Composition, Text } from '@celesta/react';

export default function Root() {
  return (
    <Composition width={1920} height={1080} fps={30} durationInFrames={150}>
      <Text
        x={960}
        y={540}
        anchorX={0.5}
        anchorY={0.5}
        style={{
          fontFamily: 'sans-serif',
          fontSize: 96,
          fill: { type: 'solid', color: '#ffffff' },
          align: 'center',
        }}
      >
        Hello from React
      </Text>
    </Composition>
  );
}
