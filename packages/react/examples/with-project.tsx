import { Composition, ProjectTimeline, Text } from '@mikan/react';

export default function Root() {
  return (
    <Composition width={640} height={360} fps={30} durationInFrames={30}>
      <ProjectTimeline />
      <Text
        x={320}
        y={40}
        style={{ fontSize: 24, fill: { type: 'solid', color: '#ffffff' } }}
      >
        React overlay
      </Text>
    </Composition>
  );
}
