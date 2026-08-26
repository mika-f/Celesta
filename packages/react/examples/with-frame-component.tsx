import { Composition, Text, registerComponent, useCurrentFrame, useVideoConfig } from '@mikan/react';

// A registered component whose output depends on the composition timeline —
// the editor's component-resolution path must give its hooks the real
// playhead time (and this entry's own <Composition> facts) so preview
// matches what an export renders at that frame.
function FrameCaption() {
  const frame = useCurrentFrame();
  const { width, fps } = useVideoConfig();
  return (
    <Text style={{ fontSize: 40, fill: { type: 'solid', color: '#ffffff' } }}>
      {`frame ${frame} of ${width} at ${fps}fps`}
    </Text>
  );
}

registerComponent('FrameCaption', FrameCaption as never);

export default function Root() {
  return (
    <Composition width={640} height={360} fps={30} durationInFrames={30}>
      <FrameCaption />
    </Composition>
  );
}
