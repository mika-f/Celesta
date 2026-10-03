import { useRef, useState } from 'react';
import {
  Assets, Audio, Character, CharacterView, Composition, Dialogue, Font, Group,
  Sequence, Series, Text, useCurrentFrame, useTextMetrics,
} from '@celesta/react';

function MeasuredSound() {
  const frame = useCurrentFrame();
  const [src] = useState('./measured.wav');
  const metrics = useTextMetrics('x'.repeat(frame % 5 + 1), { fontSize: 24 });
  return metrics.width > 30 ? <Audio src={src} volume={frame / 100} /> : null;
}

export default function Root() {
  const frame = useCurrentFrame();
  const character = useRef(null);
  const view = useRef(null);
  return (
    <Composition width={320} height={180} fps={30} durationInFrames={60}>
      <Assets>
        <Character ref={character} id="speaker" portrait={{
          type: 'image', defaultExpression: 'neutral', expressions: { neutral: './portrait.png' },
        }} />
      </Assets>
      <CharacterView ref={view} character={character} />
      <Group>
        <Audio src="./duplicate.wav" />
        <Audio src="./duplicate.wav" />
        {frame === 17 && <Audio src="https://example.com/one-frame.wav" />}
        <Sequence from={10} durationInFrames={30}>
          <Sequence from={-5} durationInFrames={20}>
            <Audio src="./clipped.wav" startFrom={0.5} playbackRate={2} muted />
          </Sequence>
          <MeasuredSound />
        </Sequence>
        <Series>
          <Series.Sequence durationInFrames={30}>
            <Dialogue character={view} audio="./dialogue.wav">Hello</Dialogue>
          </Series.Sequence>
          <Series.Sequence durationInFrames={30} offset={-5}>
            <Audio src="./animated.wav" volume={{ type: 'keyframes', keyframes: [
              { time: { value: 0, timescale: 1 }, value: 0 },
              { time: { value: 1, timescale: 1 }, value: 1, easing: 'ease-in' },
            ] }} />
          </Series.Sequence>
        </Series>
      </Group>
      {/* Changing declarations force the same extra reconciliation in both paths. */}
      {frame >= 25 && <Font src="../../../../examples/prism/assets/fonts/IBMPlexMono-Regular.ttf" />}
      <Text>{frame}</Text>
    </Composition>
  );
}
