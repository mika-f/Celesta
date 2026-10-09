import { Composition, Motion, Rect, Sequence, Theme, Title } from '@celesta/react';

export default function Root() {
  return (
    <Composition width={1280} height={720} fps={30} durationInFrames={150}>
      <Rect width={1280} height={720} fill="#20243a" />
      <Theme font="sans-serif" color="#ffffff" size={72}>
        <Sequence durationInFrames={90}>
          <Title center enter="fade">Hello, Celesta.</Title>
        </Sequence>
        <Sequence from={90} durationInFrames={60}>
          <Title center enter="slide-up">Code is the cut.</Title>
          <Motion x={640} y={440} enter={{ type: 'scale-in', durationInFrames: 20 }}>
            <Title anchorX={0.5} size={24} color="#e4daf0">Written in React.</Title>
          </Motion>
        </Sequence>
      </Theme>
    </Composition>
  );
}
