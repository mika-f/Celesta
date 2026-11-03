import { Composition, Group, Rect, Text, useCurrentFrame } from '@celesta.studio/react';

// Stand-in green-screen footage, drawn by Celesta so it can ship with the
// repository: a smiling presenter waving and holding a HELLO card before
// an unevenly lit #00b140 screen, casting a shadow on it, with a sheer
// scarf the screen shows through, a green cast on everything (spill), soft
// moving edges, and sensor grain.
// Exported to assets/footage.mp4 and assets/still.jpg; see the README.

const W = 640;
const H = 360;

export default function Footage() {
  const frame = useCurrentFrame();
  const phase = (frame / 90) * Math.PI * 4;
  const wave = Math.sin(phase);
  const speed = Math.abs(Math.cos(phase));
  const sway = Math.sin((frame / 90) * Math.PI * 2) * 6;
  return (
    <Composition width={W} height={H} fps={30} durationInFrames={90}>
      <Group noise={0.025}>
        <Screen />
        {/* The presenter's shadow on the screen, soft and to the right. */}
        <Group x={40 + sway} y={12} opacity={0.3} blur={18}>
          <Rect x={271} y={66} width={98} height={117} cornerRadius={46} fill="#003a14" />
          <Rect x={248} y={194} width={144} height={194} cornerRadius={48} fill="#003a14" />
          <Group x={408} y={235} rotation={18 + wave * 22}>
            <Rect x={-14} y={-77} width={28} height={84} cornerRadius={14} fill="#003a14" />
            <Rect x={-17} y={-139} width={37} height={66} cornerRadius={14} fill="#003a14" />
          </Group>
        </Group>
        <Group x={sway} tint={{ color: '#00b140', amount: 0.1 }}>
          <Presenter wave={wave} speed={speed} />
        </Group>
      </Group>
    </Composition>
  );
}

/** The green screen, brighter at the top centre where the lights point. */
function Screen() {
  return (
    <Rect
      width={W}
      height={H}
      fill={{
        type: 'radial',
        center: { x: W / 2, y: H * 0.3 },
        radius: W * 0.75,
        stops: [
          { offset: 0, color: '#12c450' },
          { offset: 0.6, color: '#00b140' },
          { offset: 1, color: '#008a32' },
        ],
      }}
    />
  );
}

function Presenter({ wave, speed }: { wave: number; speed: number }) {
  return (
    <>
      {/* A soft navy jacket, with seams and a warm shirt beneath. */}
      <Rect x={248} y={194} width={144} height={194} cornerRadius={48} fill="#25334d" />
      <Rect x={282} y={205} width={76} height={170} cornerRadius={20} fill="#e7ddd1" />
      <Rect x={301} y={170} width={38} height={45} cornerRadius={15} fill="#d99c79" />
      <Rect x={286} y={194} width={29} height={66} cornerRadius={5} rotation={-19} fill="#415473" />
      <Rect x={335} y={194} width={29} height={66} cornerRadius={5} rotation={19} fill="#415473" />
      <Rect x={268} y={278} width={4} height={68} cornerRadius={2} fill="#17243b" />
      <Rect x={369} y={275} width={4} height={72} cornerRadius={2} fill="#17243b" />
      <Rect x={345} y={269} width={25} height={3} fill="#8090a7" />
      <Rect x={317} y={242} width={5} height={5} cornerRadius={2.5} fill="#a47f66" />
      <Rect x={317} y={328} width={5} height={5} cornerRadius={2.5} fill="#a47f66" />
      {/* Ears, swept hair, and a smiling face. */}
      <Rect x={266} y={112} width={18} height={30} cornerRadius={9} fill="#dfa17f" />
      <Rect x={356} y={112} width={18} height={30} cornerRadius={9} fill="#dfa17f" />
      <Rect x={271} y={66} width={98} height={111} cornerRadius={46} fill="#352b2b" />
      <Rect x={280} y={84} width={80} height={99} cornerRadius={38} fill="#f1bf98" />
      <Rect x={278} y={76} width={64} height={29} cornerRadius={14} rotation={-12} fill="#352b2b" />
      <Rect x={329} y={74} width={29} height={34} cornerRadius={13} rotation={16} fill="#352b2b" />
      <Rect x={284} y={73} width={40} height={5} cornerRadius={2.5} rotation={-12} fill="#675044" />
      <Rect x={292} y={115} width={14} height={3} cornerRadius={1.5} rotation={-7} fill="#50392f" />
      <Rect x={335} y={113} width={14} height={3} cornerRadius={1.5} rotation={7} fill="#50392f" />
      <Rect x={298} y={123} width={5} height={8} cornerRadius={2.5} fill="#352b2b" />
      <Rect x={339} y={123} width={5} height={8} cornerRadius={2.5} fill="#352b2b" />
      <Rect x={318} y={126} width={7} height={17} cornerRadius={3.5} fill="#dd9c7a" />
      <Rect x={286} y={139} width={16} height={8} cornerRadius={4} fill="#eaa08b" />
      <Rect x={341} y={139} width={16} height={8} cornerRadius={4} fill="#eaa08b" />
      <Rect x={307} y={151} width={27} height={13} cornerRadius={6.5} fill="#883f3d" />
      <Rect x={310} y={151} width={21} height={5} cornerRadius={2} fill="#fff4e5" />
      {/* One sheer scarf tail falls outside the body onto the screen. */}
      <Group opacity={0.55}>
        <Rect x={283} y={191} width={76} height={20} cornerRadius={10} fill="#fff0e5" />
        <Rect x={268} y={203} width={27} height={122} cornerRadius={8} rotation={20} fill="#fff0e5" />
        <Rect x={248} y={253} width={2} height={45} rotation={20} fill="#d6bcaa" />
      </Group>
      {/* The elbow stays planted while the forearm and open hand wave. */}
      <Rect x={364} y={201} width={36} height={74} cornerRadius={18} rotation={-33} fill="#304360" />
      <Group x={408} y={235} rotation={18 + wave * 22} blur={0.3 + speed * 2.2}>
        <Rect x={-14} y={-77} width={28} height={84} cornerRadius={14} fill="#304360" />
        <Rect x={-14} y={-79} width={28} height={13} cornerRadius={4} fill="#e7ddd1" />
        <Rect x={-14} y={-111} width={28} height={37} cornerRadius={12} fill="#f1bf98" />
        <Rect x={-17} y={-132} width={7} height={32} cornerRadius={3.5} rotation={-8} fill="#f1bf98" />
        <Rect x={-7} y={-141} width={7} height={37} cornerRadius={3.5} fill="#f1bf98" />
        <Rect x={3} y={-138} width={7} height={34} cornerRadius={3.5} rotation={5} fill="#f1bf98" />
        <Rect x={13} y={-129} width={7} height={28} cornerRadius={3.5} rotation={12} fill="#f1bf98" />
        <Rect x={-27} y={-105} width={10} height={27} cornerRadius={5} rotation={-35} fill="#f1bf98" />
        <Rect x={-4} y={-94} width={14} height={2} cornerRadius={1} fill="#d99c79" />
      </Group>
      {/* The other hand supports a small printed greeting card. */}
      <Rect x={241} y={221} width={32} height={77} cornerRadius={16} rotation={17} fill="#304360" />
      <Group x={205} y={260} rotation={-6}>
        <Rect x={3} y={4} width={128} height={70} cornerRadius={8} fill="#17243b" />
        <Rect width={128} height={70} cornerRadius={8} fill="#fff5e8" />
        <Rect x={10} y={10} width={108} height={50} cornerRadius={4} stroke="#dfb99b" strokeWidth={1} />
        <Text
          x={64}
          y={35}
          anchorX={0.5}
          anchorY={0.5}
          style={{ fontFamily: 'sans-serif', fontSize: 27, fontWeight: 800, fill: { type: 'solid', color: '#bd493f' } }}
        >
          HELLO
        </Text>
        <Rect x={9} y={56} width={30} height={15} cornerRadius={7} fill="#f1bf98" />
        <Rect x={14} y={61} width={19} height={2} cornerRadius={1} fill="#d99c79" />
      </Group>
    </>
  );
}
