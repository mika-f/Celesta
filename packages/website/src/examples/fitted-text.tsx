import { Composition, TextBox } from '@celesta/react';

export default function Root() {
  return (
    <Composition width={1920} height={1080} fps={30} durationInFrames={90} lang="ja-JP">
      <TextBox x={160} y={360} width={1600} height={360}
        minFontSize={40} maxFontSize={120} maxLines={2}
        verticalAlign="middle" overflow="error"
        style={{ align: 'center', fill: { type: 'solid', color: '#ffffff' } }}>
        コードで、動画を作る。
      </TextBox>
    </Composition>
  );
}
