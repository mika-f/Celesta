import { Composition, ProjectTrack, Text, useProjectTrack } from '@mikan/react';

function TitlesTrackSummary() {
  const layers = useProjectTrack('titles');
  return (
    <Text style={{ fontSize: 24, fill: { type: 'solid', color: '#00ff00' } }}>
      {`titles track has ${layers.length} layer(s)`}
    </Text>
  );
}

export default function Root() {
  return (
    <Composition width={640} height={360} fps={30} durationInFrames={30}>
      <TitlesTrackSummary />
      <ProjectTrack id="overlays" />
    </Composition>
  );
}
