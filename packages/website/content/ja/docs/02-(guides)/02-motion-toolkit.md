シーンを並べる、リストを順に表示する、拍に合わせて切り替えるなど、よく使うパターンのヘルパーです。`Sequence`、`Group`、フレームに基づくので、スクラブでき、結果も一定です。テキスト効果・カメラ・線は[次のページ](/ja/docs/text-camera-lines/)、シード付き乱数とノイズは [`@celesta/math`](/ja/docs/math/) で説明します。

## シーンを続けて並べる

`<Series>` は子の `<Series.Sequence>` を順番に再生するので、開始フレームではなく各シーンの長さを指定できます。`offset` を負にすると前のシーンと重なります。`computeSeries()` は描画せず同じ計算を行い、コンポジション全体の長さを求めるのに便利です。

```tsx title="各シーンの長さから全体を計算"
const SCENES = [
  { name: 'intro', durationInFrames: 90, Scene: Intro },
  { name: 'body', durationInFrames: 240, Scene: Body },
  { name: 'outro', durationInFrames: 60, Scene: Outro },
];
const { durationInFrames } = computeSeries(SCENES);

export default function Root() {
  return (
    <Composition width={1920} height={1080} fps={30} durationInFrames={durationInFrames}>
      <Series>
        {SCENES.map(({ name, durationInFrames, Scene }) => (
          <Series.Sequence key={name} durationInFrames={durationInFrames}>
            <Scene />
          </Series.Sequence>
        ))}
      </Series>
    </Composition>
  );
}
```

## シーンをトランジションでつなぐ

`<TransitionSeries>` は `Series` と同じくシーンを順に並べ、2 つのシーンの間に `<TransitionSeries.Transition>` を置けます。トランジションは前後のシーンを `durationInFrames` の分だけ重ねるので、`computeTransitionSeries()` で各シーンの開始位置と、重なりの分だけ短くなった全体の長さを求められます。

```tsx file=examples/transition-series.tsx title="クロスフェードとスライドでつないだシーン"
```

各シーンは通常の Sequence で、入りのトランジションの最初のフレームがフレーム 0 です。後のシーンが手前に描かれます。前後どちらかのシーンより長いトランジションや、入りと出のトランジションを合わせた長さに満たないシーンはエラーになるので、空白や 3 つのシーンの同時表示は起こりません。重なったシーンの音声は同時に再生されます。`useTransitionVolume(volume)` はシーン内の Audio に映像と一緒にフェードする音量を返し、`useTransitionSeriesScene()` はほかのタイミング調整のためにシーン前後のトランジションを返します。

`type` には次の名前を指定します。名前だけなら括弧内の既定値を使います。ほかの値にするときは、`@celesta/transitions` の同名の関数を `type={iris({ center: { x: 0.3, y: 0.5 }, feather: 40 })}` のように呼びます。`from`（既定は `'left'`）は `'slide'`・`'wipe'`・`'cube'` で次のシーンが来る辺、`'flip'` で回る軸です。`easing` はどの種類でも進み方を変えます。

| `type` | 動き | オプション |
| --- | --- | --- |
| `'cut'` | 重ねずに次のシーンへ切り替えます。 | |
| `'crossfade'` | 次のシーンを前のシーンの上にフェードインします。 | |
| `'slide'` | 次のシーンが前のシーンを押し出します。 | |
| `'wipe'` | 動く直線の境界の向こうに次のシーンを見せます。 | |
| `'iris'` | 広がる円の中に次のシーンが現れます。 | `center`（`{ x: 0.5, y: 0.5 }`、大きさに対する割合）、`direction`（`'open'`。`'close'` で前のシーンが縮む円に吸い込まれる）、`feather`（0 px） |
| `'clock-wipe'` | 時計の針のように 1 周して次のシーンを見せます。 | `center`、`startAngle`（0。12 時から時計回りの度）、`direction`（`'clockwise'` か `'counterclockwise'`） |
| `'zoom'` | 両方のシーンが点を中心に拡縮し、次のシーンがフェードインします。 | `direction`（`'in'` か `'out'`）、`scale`（1.5）、`center` |
| `'dissolve'` | 次のシーンがマスごとに現れます。 | `cellSize`（16 px）、`seed`（0） |
| `'blur'` | 前のシーンがぼけて消え、次のシーンのぼけが取れながら現れます。 | `amount`（32 px、最大 64） |
| `'flip'` | カードのように裏返ります。 | `shade`（0.4）：横を向いた面の暗さ |
| `'cube'` | 立方体の 2 面が 4 分の 1 回転します。 | `shade`（0.4） |

`'flip'` と `'cube'` は遠近法を使わずに描きます。面は細くなりながら暗くなり、立方体は回転の途中で縮んで常にシリーズの幅に収まります。どの種類もプレビュー、書き出し、ブラウザのエディタで同じように描かれます。

トランジションの動きや切り抜きは、シリーズの大きさを基準にします。スライドで押し出す距離、ワイプの終わる位置、アイリスの中心がこれで決まります。既定ではコンポジションの大きさ、`<SafeArea>` や `<Fit>` の中ではその領域の大きさです。小さな枠の中で使うときは、`<TransitionSeries>` に枠の `width` と `height` を指定し、`Group` で配置します（シーンが枠の外にはみ出さないようにするには `clip` も指定します）。

```tsx title="幅 560 px の枠の中のシリーズ"
<Group x={260} y={400} clip={{ width: 560, height: 315 }}>
  <TransitionSeries width={560} height={315}>…</TransitionSeries>
</Group>
```

### トランジションを定義する

`defineTransition({ name, present })` で `type` に渡すトランジションを作れます。`present` は重なった各フレームで、イージング後の `progress`、重なりの中の `frame` と `durationInFrames`、`from`、シリーズの `width` と `height` を受け取ります。そして、次のシーン（`enter`）と前のシーン（`exit`）の `Group` の props を返します。位置、拡縮、透明度、`clip`、`blur`、色補正、`shader`、`mask` を指定できます。組み込みの種類も同じ仕組みで書かれており、`useTransitionSeriesScene()` は定義したトランジションを `name` で返します。カスタム WGSL シェーダーはプレビューと書き出しで動き、ブラウザのエディタでは動きません。

```tsx file=examples/custom-transition.tsx title="マスクで定義したブラインドと、中心をずらしたアイリス"
```

## 順番に入場させる

`progress(frame, start, durationInFrames, easing?)` は「区間のどこまで進んだか」を 0〜1 に制限した値で返し、多くの入場効果の基礎になります。`<Stagger each={n}>` は前の子から `n` フレーム遅れて次の子を開始するので、フレーム 0 向けの入場効果がリストを順に流れます。

```tsx title="一行ずつ表示する"
function Row({ label, y }: { label: string; y: number }) {
  const frame = useCurrentFrame(); // 0 when this row starts
  const p = progress(frame, 0, 20, Easings.easeOutExpo);
  return <Text x={120 + 40 * (1 - p)} y={y} opacity={p}>{label}</Text>;
}

<Stagger each={4}>
  {items.map((item, i) => <Row key={item} label={item} y={200 + i * 64} />)}
</Stagger>
```

## 拍に合わせる

`useBeat({ bpm })` は現在の `beat`、`bar`、`beatInBar`、拍内の `progress`、拍ごとに 1 になり次の拍まで減衰する `pulse` を返します。音楽に合わせた点滅・拡縮・カットに使えます。`offset` で最初の拍をずらせます。

```tsx title="音楽に合わせて脈動する点"
const { pulse } = useBeat({ bpm: 120 });
return <Rect width={40} height={40} cornerRadius={20} scale={1 + 0.3 * pulse} fill="#7cf29c" />;
```

## キュー

`useCue(cues)` は `at` 順の `{ at, …data }` オブジェクトのリストを受け取り、現在のキュー、`index`、開始からの `frame`、`previous` と `next` のキューを返します。キャプションの切り替え、カメラの移動、グラフの注釈をキューの列で表せます。`cueAt(cues, frame)` はコンポーネント外でも同じ計算を行います。

```tsx title="キャプションを切り替えてフェードイン"
const active = useCue([
  { at: 0, text: 'Write it.' },
  { at: 45, text: 'Preview it.' },
  { at: 90, text: 'Ship it.' },
]);
if (!active) return null;
return <Text opacity={progress(active.frame, 0, 10)}>{active.cue.text}</Text>;
```

## タイムコード

`@celesta/react` の `frameToTimecode(frame, fps)` は、`HH:MM:SS:FF` 形式の画面内時計を生成します。
