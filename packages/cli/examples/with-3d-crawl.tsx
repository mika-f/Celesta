import { Composition, Group, Rect, Text, useCurrentFrame } from '@celesta.studio/react';

// A text crawl receding into the distance. One `Text` layer, 1200 wide and
// about 20000 tall, tilted back with `rotationX={70}` under a
// `perspective` of 800, scrolls away from the viewer.
//
// A plane this long cannot be drawn at one sharpness: at the scale its
// near edge needs, it would be far larger than any GPU texture. It is
// drawn in bands of equal depth instead, each as sharp as its own nearest
// part, so the lines up close stay crisp while the far ones shrink toward
// the horizon. A text layer too large for one texture is also rasterized
// only where it shows.
const WIDTH = 1920;
const HEIGHT = 1080;

const PARAGRAPH = [
  'It is a period of quiet renders.',
  'Frames stream from the timeline',
  'through every band of the plane,',
  'each drawn as sharp as its own',
  'nearest line, while the far rows',
  'fade into the distance.',
  '',
].join('\n');

// Sixty paragraphs, seven lines each at 48 per line: about 20000 tall.
const CRAWL = Array.from({ length: 60 }, (_, i) => `Episode ${i + 1}\n${PARAGRAPH}`).join('\n');

export default function Root() {
  const frame = useCurrentFrame();
  return (
    <Composition width={WIDTH} height={HEIGHT} fps={30} durationInFrames={240}>
      <Rect width={WIDTH} height={HEIGHT} fill="#05060B" />
      <Group x={WIDTH / 2} y={900} rotationX={70} perspective={800}>
        <Text
          x={0}
          y={1500 - frame * 6}
          anchorX={0.5}
          anchorY={1}
          maxWidth={1200}
          style={{
            fontFamily: 'sans-serif',
            fontSize: 40,
            fontWeight: 700,
            align: 'center',
            fill: { type: 'solid', color: '#F2C94C' },
          }}
        >
          {CRAWL}
        </Text>
      </Group>
    </Composition>
  );
}
