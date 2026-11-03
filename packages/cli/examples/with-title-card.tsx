import { PatternBackground, TitleCard } from '@celesta.studio/components';
import { Composition, Font, Sequence, Theme } from '@celesta.studio/react';

// <TitleCard> stacks a chapter number, kicker, title, and subtitle inside
// the canvas or a safe area, fits the title to the room left, and brings
// the pieces in through <Motion>. The first card staggers them (the
// default); the second is the calm setting: centred, one fade, no stagger.
// The vertical composition keeps a long title clear of the platform UI
// while its background fills the whole frame.

const colors = { paper: '#F4F1EA', ink: '#111111', accent: '#FF4D1F', soft: '#9A968E' };
const fonts = { display: 'Inter', ja: 'Noto Sans JP' };

function Fonts() {
  return <>
    <Font google="Inter" weights={[400, 700]} />
    <Font google="Noto Sans JP" weights={[400, 700]} />
  </>;
}

export default function WithTitleCard() {
  return <>
    <Composition id="landscape" width={960} height={540} fps={30} durationInFrames={150}>
      <Fonts />
      <Theme fonts={fonts} colors={colors}>
        <Sequence durationInFrames={90}>
          <TitleCard chapter="01" kicker="GETTING STARTED" kickerRule title={'Code is\nthe cut'}
            subtitle="Celesta 入門 — React で書いて、そのまま書き出す" fontFamily={['Inter', 'Noto Sans JP']}
            background="ink" color="paper" subtitleColor="soft" accentColor="accent" />
        </Sequence>
        <Sequence from={90} durationInFrames={60}>
          <TitleCard title="Thank you for watching" subtitle="celesta.dev" align="center"
            enter="fade" stagger={0} fontFamily="display" color="ink" subtitleColor="soft"
            background={<>
              <PatternBackground pattern="dots" spacing={32} size={3} background={colors.paper} color={colors.ink} opacity={0.08} />
            </>} />
        </Sequence>
      </Theme>
    </Composition>
    <Composition id="vertical" width={1080} height={1920} fps={30} durationInFrames={90}>
      <Fonts />
      <Theme fonts={fonts} colors={colors}>
        <TitleCard safeArea="vertical-social" verticalAlign="bottom" kicker="SHORTS · 02" kickerRule maxTitleLines={4}
          title="長いタイトルは列に収まるまで文字を小さくして、折り返して置きます"
          subtitle="縦の画角でも、文字だけが UI を避け、背景は画面いっぱいに広がります。"
          fontFamily="ja" background="accent" color="paper" accentColor="ink" />
      </Theme>
    </Composition>
  </>;
}
