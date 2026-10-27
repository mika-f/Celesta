import { Composition, Instances } from '@celesta/react';

export default function InvalidInstanceFill() {
  return (
    <Composition width={10} height={10} fps={30} durationInFrames={1}>
      <Instances count={1} x={[1]} y={[1]} width={1} height={1} fill="#aéaaa" />
    </Composition>
  );
}
