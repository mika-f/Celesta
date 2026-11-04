import { TypewriterText } from '@celesta.studio/components';
import { Composition, Font, Rect, Span, Text, Theme } from '@celesta.studio/react';

// <TypewriterText> types its text out with a caret after it, timed as
// `useTypewriter()` times it. The caret goes where the measured layout puts
// the next character, so it needs no monospaced font: it follows Inter's
// narrow `i` and wide `W`, Japanese, `\n`, and lines wrapped at `maxWidth`.
// Before typing starts and once it is done, the caret blinks.

const fonts = { mono: 'JetBrains Mono', sans: ['Inter', 'Noto Sans JP'] };
const colors = { bg: '#101820', ink: '#F4F1EA', dim: '#8A94A0', accent: '#FF4D1F', mint: '#7CE0C3' };

function Label({ x = 120, y, children }: { x?: number; y: number; children: string }) {
  return <Text x={x} y={y} fontFamily="mono" fontSize={16} fontWeight={700} color="dim">{children}</Text>;
}

export default function WithTypewriterText() {
  return (
    <Composition width={1920} height={1080} fps={30} durationInFrames={180}>
      <Font google="JetBrains Mono" weights={[400, 700]} />
      <Font google="Inter" weights={[400, 700]} />
      <Font google="Noto Sans JP" weights={[400]} />
      <Theme fonts={fonts} colors={colors}>
        <Rect width={1920} height={1080} fill={colors.bg} />

        <Label y={90}>BLOCK CARET, MONOSPACED</Label>
        <TypewriterText x={120} y={130} from={10} framesPerChar={3} caretShape="block" caretColor="mint"
          fontFamily="mono" fontSize={44} color="ink">
          <Span color="mint" fontWeight={700}>$</Span> pnpm run build
        </TypewriterText>

        <Label y={270}>LINE CARET, PROPORTIONAL</Label>
        <TypewriterText x={120} y={310} from={20} framesPerChar={1.5} caretColor="accent"
          fontFamily="sans" fontSize={64} fontWeight={700} color="ink">
          Will it fit? iiii WWWW
        </TypewriterText>

        <Label y={470}>JAPANESE, \n AND maxWidth</Label>
        <TypewriterText x={120} y={510} from={10} framesPerChar={2} maxWidth={760} lang="ja"
          fontFamily="sans" fontSize={44} color="ink" style={{ lineHeight: 66 }}>
          {'日本語の文章も、指定した幅で折り返しながら一文字ずつ入力します。\n改行のあとも続きます。'}
        </TypewriterText>

        <Label x={1120} y={470}>CENTERED</Label>
        <TypewriterText x={1120} y={510} maxWidth={640} from={40} framesPerChar={2}
          fontFamily="sans" fontSize={44} color="ink" caretColor="accent"
          style={{ align: 'center', lineHeight: 60 }}>
          {'Centered lines\nkeep their places\nwhile they type.'}
        </TypewriterText>
      </Theme>
    </Composition>
  );
}
