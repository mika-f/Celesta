import { LowerThird } from '@celesta.studio/components';
import { Composition, Font, Rect, Sequence, Theme } from '@celesta.studio/react';

// <LowerThird> puts a name and a role on a band sized to them, in a corner
// of the canvas or a safe area. The band slides in toward the middle of the
// picture and the lines follow it; everything fades out at the end of the
// enclosing <Sequence>, which sets how long the caption stays. A long name
// wraps and shrinks to its line limit; the vertical composition keeps the
// band clear of the platform UI.

const colors = { night: '#16202E', sky: '#60A0FF', amber: '#FFB347', paper: '#F4F1EA' };
const fonts = { ui: ['Inter', 'Noto Sans JP'] };

function Fonts() {
  return <>
    <Font google="Inter" weights={[400, 700]} />
    <Font google="Noto Sans JP" weights={[400, 700]} />
  </>;
}

export default function WithLowerThird() {
  return <>
    <Composition id="landscape" width={1280} height={720} fps={30} durationInFrames={180}>
      <Fonts />
      <Theme fonts={fonts} colors={colors}>
        <Rect width={1280} height={720} fill={colors.night} />
        <Sequence from={10} durationInFrames={80}>
          <LowerThird title="夏猫" subtitle="ソフトウェアエンジニア" fontFamily="ui" accent="sky" />
        </Sequence>
        <Sequence from={100} durationInFrames={70}>
          <LowerThird placement="bottom-right" title="Celesta Contributors" fontFamily="ui" accent="amber"
            background="#0B1220E6" cornerRadius={8} />
        </Sequence>
      </Theme>
    </Composition>
    <Composition id="vertical" width={1080} height={1920} fps={30} durationInFrames={90}>
      <Fonts />
      <Theme fonts={fonts} colors={colors}>
        <Rect width={1080} height={1920} fill={colors.night} />
        <LowerThird safeArea="vertical-social" title="とても長い名前の登壇者がいても二行に収まるまで小さくなります"
          subtitle="縦の画角では、帯はセーフエリアの幅いっぱいまで広がります" fontFamily="ui" accent="sky" />
      </Theme>
    </Composition>
  </>;
}
