コンポジションの秒数は `durationInFrames / fps` です。30 fps では 150 フレームで 5 秒になり、番号は 0〜149 です。コンポジション内の `useCurrentFrame()` でアニメーションの値を計算します。

## 値を補間する

タイトルのサンプルは透明度に `Math.min(frame / 30, 1)` を使っています。イージングなどを制御するには、`@celesta.studio/react` から `interpolate` と `Easings` をインポートし、計算を以下に置き換えます。

```tsx title="イージング付きのフェード（コンポーネント内）"
const opacity = interpolate(frame, [0, 30], [0, 1], {
  easing: Easings.easeOut,
  extrapolateLeft: 'clamp',
  extrapolateRight: 'clamp',
});
```

結果を `<Text opacity={opacity} … />` に渡します。入力範囲は単調増加で、出力範囲と要素数が同じ必要があります。入力値と範囲の各要素は有限の数値でなければなりません。綴りを誤ったフィールドなどから生じた `NaN` は、`NaN` を返さずにエラーになります。外挿の既定値は `extend` です。最初と最後の値を保持するには `clamp` を使います。

`Easings` は easings.net の曲線に対応します。`linear` と、`Sine`、`Quad`、`Cubic`、`Quart`、`Quint`、`Expo`、`Circ`、`Back`、`Elastic`、`Bounce` の `easeIn` / `easeOut` / `easeInOut`（例：`Easings.easeOutBack`）が使えます。

## 色を補間する

`@celesta.studio/react` から `interpolateColor` もインポートします。`interpolate` と同じ入力範囲と `easing` を取り、フレームを `#RRGGBB` または `#RRGGBBAA` の色に対応付けて `#RRGGBBAA` を返します。`Rect` の塗り、テキストの塗り、グラデーションの色、影などに使えます。

```tsx title="夜から昼へ、タイトルをフェードイン"
const sky = interpolateColor(frame, [0, 45, 90], ['#101820', '#EF7B45', '#7FC8F8']);
const title = interpolateColor(frame, [0, 20], ['#FFFFFF00', '#FFFFFF']);

<Rect width={1920} height={1080} fill={sky} />
<Text style={{ fontSize: 96, fill: { type: 'solid', color: title } }}>Morning</Text>
```

`interpolate` と違い、外挿の既定値は `clamp` で範囲外の色を保持します。`extend` は端の区間を延長し、`identity` は使えません。色はグラデーションや CSS と同じく、乗算済みアルファを使って sRGB で混ぜます。`#00000000` へのフェードでも暗くならず色相が保たれます。`extend` やオーバーシュートするイージングで範囲外になるチャンネルは制限されます。CSS の色名、`rgb()`、短縮形の `#RGB` は使えません。

## 少し弾ませる

`spring()` は減衰するばねを再現します。0 からはじまり、途中で行き過ぎることもあり、1 に落ち着きます。フレームレートが必要なので `useVideoConfig()` から取得します。

```tsx title="ばねのようにポップイン"
function Badge() {
  const frame = useCurrentFrame();
  const { fps } = useVideoConfig();
  const scale = spring({ frame, fps, delay: 10, config: { damping: 12 } });
  return (
    <Rect x={960} y={540} anchorX={0.5} anchorY={0.5} scale={scale}
      width={320} height={120} cornerRadius={60} fill="#78618e" />
  );
}
```

`config` の `stiffness`（既定 100）、`damping`（既定 10）、`mass`（既定 1）、`overshootClamping` で動きを調整します。`from` と `to` で別の範囲に変換できます。`frame`・`delay`・`from`・`to` は有限の数値、`fps` は 0 より大きい有限の数値でなければならず、それ以外や `NaN` の `durationInFrames` はエラーになります。

## シーンをタイムラインに配置

`Sequence` をインポートしてレイヤーのグループを遅延・制限します。以下は前のサンプルの `Title` をフレーム 30 から 90 フレーム表示します。

```tsx title="Composition の内側"
<Sequence from={30} durationInFrames={90}>
  <Title />
</Sequence>
```

シーケンス内の `useCurrentFrame()` は開始時に 0 になります。子の映像と音声も同じローカル時計を使います。シーケンスの時間範囲外では子のレイヤーも音声も出力されません。

`from` の既定値は 0 で、指定するなら有限の数値です（負の数も使えます）。`durationInFrames` を省略するか `Infinity` にすると、外側のシーケンスまたはコンポジションの終わりまで続きます。`0` は空の範囲です。`NaN`・無限大・数値以外の `from` と、`NaN`・負の数・数値以外の `durationInFrames` は、プロップ名を示すエラーになります。綴りを誤ったフィールドから計算したタイミングは、後のシーンすべてに重なる代わりにエラーで止まります。

## シーンを繰り返す

`<Loop durationInFrames>` は子要素を繰り返し再生します。`durationInFrames` フレームごとに子の時計が 0 に戻ります。背景アニメーションや短い BGM の繰り返しに使えます。

```tsx title="2 秒の明滅と 4 秒の BGM フレーズを繰り返す"
<Loop durationInFrames={60}>
  <Pulse />
</Loop>
<Loop durationInFrames={120} times={3}>
  <Audio src="./phrase.wav" />
</Loop>
```

内側の `useCurrentFrame()`、`<Video>`、`<Audio>`、入れ子の Sequence は周期ごとにそろって最初に戻り、`useVideoConfig().durationInFrames` は 1 周期の長さです。`from` で最初の周期の開始位置を `Sequence` と同じように指定します。`times` を省くと、囲んでいる Sequence かコンポジションの終わりまで繰り返します。`2.5` のような端数の `times` では最後の周期が途中で切れます。音声は周期ごとに別のクリップになり、ループの終わりで切れます。マウントされるのは現在の周期だけなので、どのフレームへスクラブしても書き出しと同じ絵になります。

## 過去のフレームを再描画

各フレームはフレーム番号の関数なので、`<FreezeFrame frame>` で動画の任意の時点をその場に再描画できます。巻き戻し、回想、サムネイルの列などに使えます。`frame` は `Sequence` 内でもコンポジション全体の時計で数えます。

```tsx title="フレーム 1200 を 4 分の 1 のサイズに"
<Group scale={0.25} x={80} y={80}>
  <FreezeFrame frame={1200}>
    <World />
  </FreezeFrame>
</Group>
```

直下の `useCurrentFrame()` は整数に丸めた `frame` を返します（その内側の `Sequence` は自身の開始から数えます）。シーケンス、映像、口パク、まばたきもその時点を表示します。固定の `frame` は静止し、ルートの `useCurrentFrame()` から取得した `frame` で `frame={frame - 90}` を使うと 90 フレーム前（30 fps なら 3 秒前）を再生します。固定された子は無音です。内側の `CharacterView` の ref はライブ側のビューを指し、内側の `Dialogue` は同じ `FreezeFrame` 内のビューだけを操作します。プロジェクトのレイヤー（`ProjectTimeline`、`ProjectTrack`）は現在のフレームでしか評価されず、内側には描画できません。映像の中に自分自身の縮小版を描くことはできますが、`FreezeFrame` の `frame` がその `FreezeFrame` 自身を再び描く時点だと無限に描き続けることになるため、関わったフレームを示すエラーになります。`FreezeFrame` を 16 段より深く入れ子にした場合もエラーになります。

## 速い動きをぼかす

2 つのフレームの間で大きく動くものは、ある位置から次の位置へ飛んで見えます。`<MotionBlur>` は、フレームを中心にシャッターが開いている間の複数の時点で子要素を描き、その平均を表示します。カメラで撮ったときのように動きが流れます。

```tsx title="横切るカードをぼかす"
<MotionBlur shutterAngle={180} samples={8}>
  <DashingCard />
</MotionBlur>
```

`shutterAngle` はシャッターが開いている長さで、1 フレームを 360 度として表します。180（既定）はフレームを中心とした半フレーム、360 は 1 フレーム分です。`samples`（既定 8）は平均する時点の数です。時点ごとに子要素をもう一度描くので、ブラーの重さは子要素のおよそその倍数になります。とても速い動きが滑らかに流れず段になって見えるときは、`samples` を増やしてください。

内側の `useCurrentFrame()` は `12.25` のような各時点のフレームを返すので、そこから動きを計算しているコンポーネントは時点ごとに動きます。フレームで配列を引くコードには、そこで `Math.floor` が必要です。外側で計算した props（`Camera` の `x` など）はどの時点でも同じ値なので、時点ごとに計算するには関数を渡します。

```tsx title="カメラの移動をぼかす"
<MotionBlur>
  {(frame) => (
    <Camera x={interpolate(frame, [0, 30], [0, 3000], { extrapolateRight: 'clamp' })}>
      <World />
    </Camera>
  )}
</MotionBlur>
```

あるフレームの結果は、その前にどのフレームを描いたかに左右されません。エディタのプレビューも書き出しと同じブラーを描きます。時点は囲んでいる `Sequence` の範囲に収めるので、ショットの最初と最後のフレームが薄くなることはありません。`<Video>`、アニメーション画像、口パク、まばたきは、どの時点でもそのフレームの絵を表示します（素材の映像には撮影時のブラーがすでに含まれています）。内側の音声と字幕は 1 回分だけ出力されます。各時点は `FreezeFrame` と同じく複製なので、内側の `CharacterView` は ref を付けません。その `Dialogue` も内側に置いてください。プロジェクトのレイヤーは内側に描画できません。ぼかしたレイヤーはグループの props（`x`、`opacity`、`blendMode`、`blur` など）を受け取り、それらは平均した結果にかかります。

## 残像を残す

`<Trail>` は子要素の下に、上のものより `lagInFrames` フレームずつ古く、薄い複製を描きます。

```tsx title="4 つの残像を引くボール"
<Trail copies={4} lagInFrames={1.5} trailOpacity={0.5}>
  <Ball />
</Trail>
```

いちばん新しい複製は `trailOpacity` で描かれ、古くなるごとに同じ幅ずつ薄くなります。複製はその不透明度を持つ `Group` と同じように薄くなるので、1 つの複製の中で重なったレイヤーは互いに透けて見えます。囲んでいる `Sequence` が始まる前の複製は描きません。複製の中の `useCurrentFrame()` と、映像・音声・キャラクターの扱いは `MotionBlur` と同じです。いちばん上の子要素は現在のフレームのものです。

## 入場と退場のアニメーション

`<Transition>` は親シーケンスの開始（`direction="in"`、既定）または終了（`direction="out"`）で、子をフェード・スライド・拡縮します。

```tsx title="キャプションをスライドインしてフェードアウト"
<Sequence from={30} durationInFrames={120}>
  <Transition type="slide" slideFrom="bottom" durationInFrames={15}
    easing={Easings.easeOutCubic}>
    <Transition type="fade" direction="out" durationInFrames={20}>
      <Caption />
    </Transition>
  </Transition>
</Sequence>
```

スライドの距離は `distance` ピクセル（既定 64）、拡縮の開始倍率は `scaleFrom`（既定 0.8）です。`type={['fade', 'slide']}` のように複数の種類を組み合わせられます。

> [!NOTE]
> **フレームに基づくアニメーション**
> フレームや時間から値を計算すると、スクラブと書き出しで同じ瞬間を再現できます。ブラウザのタイマーや `Math.random()` はコンポジションの時計ではありません。ランダムな値もフレームやインデックスから、[`@celesta.studio/math`](/ja/docs/math/) のシード付き `random` と `noise` で導いてください。

## 指定したフレームで静止させる

`<FreezeFrame frame={60}>` は、Sequence の内側でも、コンポジション全体の 60 フレーム目の状態で子要素を描きます。フック、映像、口パク、まばたきがその時刻に従い、内側の Sequence には引き続き開始位置のずれが適用されます。FreezeFrame 内の音声はミュートされます。

```tsx title="FreezeFrame"
<FreezeFrame frame={60}>
  <AnimatedTitle />
</FreezeFrame>
```
