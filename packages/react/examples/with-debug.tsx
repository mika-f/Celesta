import {
  Composition,
  DebugBounds,
  DebugOverlay,
  Rect,
  registerComponent,
} from '@celesta/react';

function DebugCard() {
  return (
    <>
      <DebugOverlay safeArea={20} />
      <DebugBounds width={160} height={90} label="card">
        <Rect width={160} height={90} fill="#303846" cornerRadius={8} />
      </DebugBounds>
    </>
  );
}

registerComponent('DebugCard', DebugCard as never);

export default function Root() {
  return (
    <Composition width={640} height={360} fps={30} durationInFrames={30}>
      <DebugOverlay />
    </Composition>
  );
}
