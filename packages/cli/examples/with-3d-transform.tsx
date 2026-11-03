import { Composition, Group, Rect, Text, useCurrentFrame } from '@celesta.studio/react';
import { Circle, Star } from '@celesta.studio/shapes';

// Skew and 3D transforms. Every layer and group takes `skewX`/`skewY`,
// `rotationX`/`rotationY`, a `perspective` distance, and
// `backfaceVisibility`. Skew and a rotation without perspective only
// reshape the layer; with `perspective` it recedes in depth.
//
// Top left: a card that turns over once every three seconds. The group
// turns about its origin (`rotationY`, with a `perspective` of 1400) and
// each face hides when it faces away (`backfaceVisibility="hidden"`). The
// back is a group turned 180 degrees, so it reads correctly once the card
// has turned.
// Top right: cards leaning with `skewX`.
// Bottom: a floor of tiles tilted back with `rotationX` and a
// `perspective` of 900. The nearest row stays sharp, the far rows shrink
// without shimmering. Children stay flat in their parent's plane: there is
// no depth sorting, they draw in painter's order.
const WIDTH = 1920;
const HEIGHT = 1080;
const INK = '#F4EFE6';

const label = (fontSize: number, color: string, fontWeight = 700) => ({
  fontFamily: 'sans-serif',
  fontSize,
  fontWeight,
  align: 'center' as const,
  fill: { type: 'solid' as const, color },
});

/** A card that turns over continuously; its two faces hide when facing away. */
function FlippingCard({ angle }: { angle: number }) {
  return (
    <Group x={480} y={320} rotationY={angle} perspective={1400}>
      <Group backfaceVisibility="hidden">
        <Rect x={-170} y={-235} width={340} height={470} cornerRadius={28} fill="#F4EFE6" />
        <Text x={0} y={-150} anchorX={0.5} anchorY={0.5} style={label(64, '#1D3557', 800)}>
          Front
        </Text>
        <Star x={0} y={30} anchorX={0.5} anchorY={0.5} radius={110} points={5} fill="#E63946" />
        <Text x={0} y={190} anchorX={0.5} anchorY={0.5} style={label(30, '#1D3557', 500)}>
          turns over
        </Text>
      </Group>
      <Group rotationY={180} backfaceVisibility="hidden">
        <Rect x={-170} y={-235} width={340} height={470} cornerRadius={28} fill="#1D3557" />
        <Text x={0} y={-150} anchorX={0.5} anchorY={0.5} style={label(64, INK, 800)}>
          Back
        </Text>
        <Circle x={0} y={30} anchorX={0.5} anchorY={0.5} radius={100} fill="#A8DADC" />
        <Text x={0} y={190} anchorX={0.5} anchorY={0.5} style={label(30, INK, 500)}>
          reads correctly
        </Text>
      </Group>
    </Group>
  );
}

/** Three cards leaning to the left. */
function LeaningCards() {
  const colors = ['#E63946', '#F2B134', '#4DD8C0'];
  return (
    <Group x={1030} y={150}>
      {colors.map((color, i) => (
        <Group key={color} x={i * 290} skewX={-12}>
          <Rect width={250} height={340} cornerRadius={20} fill={color} />
          <Text x={125} y={170} anchorX={0.5} anchorY={0.5} style={label(48, '#101820', 800)}>
            {`skewX\n-12`}
          </Text>
        </Group>
      ))}
    </Group>
  );
}

/** A floor of tiles tilted back; the top edge recedes. */
function Floor() {
  const columns = 12;
  const rows = 7;
  const size = 150;
  return (
    <Group x={WIDTH / 2} y={800} rotationX={60} perspective={900}>
      {Array.from({ length: rows * columns }, (_, i) => {
        const column = i % columns;
        const row = Math.floor(i / columns);
        const x = (column - columns / 2) * size;
        const y = (row - rows + 2) * size;
        const dark = (column + row) % 2 === 0;
        return (
          <Group key={i} x={x} y={y}>
            <Rect width={size - 6} height={size - 6} fill={dark ? '#2B3A55' : '#3F5378'} />
            <Text x={(size - 6) / 2} y={(size - 6) / 2} anchorX={0.5} anchorY={0.5}
              style={label(48, INK, 600)}>
              {`${String.fromCharCode(65 + row)}${column + 1}`}
            </Text>
          </Group>
        );
      })}
    </Group>
  );
}

export default function Root() {
  const frame = useCurrentFrame();
  return (
    <Composition width={WIDTH} height={HEIGHT} fps={30} durationInFrames={180}>
      <Rect width={WIDTH} height={HEIGHT} fill="#101820" />
      <Floor />
      <FlippingCard angle={frame * 4} />
      <LeaningCards />
    </Composition>
  );
}
