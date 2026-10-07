import { Composition, Text } from '@celesta/react';

const settings = { title: 'Chapter 3', accent: '#a68bbf' };
type TitleProps = { title: string; accent: string };

function Title({ title, accent }: TitleProps) {
  return (
    <Text x={960} y={540} anchorX={0.5} anchorY={0.5}
      style={{ fontSize: 96, fill: { type: 'solid', color: accent } }}>
      {title}
    </Text>
  );
}

export default function Root() {
  return (
    <Composition width={1920} height={1080} fps={30} durationInFrames={90}>
      <Title {...settings} />
    </Composition>
  );
}
