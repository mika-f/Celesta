表示されていくタイトル、移動するカメラ、描かれていく線。前のページのヘルパーと同様にフレームに基づくので、スクラブと書き出しで各瞬間を再現できます。

## テキスト効果

**テキストのヘルパー**

| API | 用途 |
| --- | --- |
| `TextReveal` | 文字列の各行を、それぞれのマスクの裏から順にスライド表示します。`lineHeight`、`from`、`stagger`、`durationInFrames` を設定し、`align` で各行の横方向の揃え位置（0 左、0.5 中央、1 右）を指定します。行は `\n` で区切ります。`maxWidth` を渡すと `<Text maxWidth>` と同じ位置（`style.lineBreak: 'phrase'` を含む）でも折り返し、折り返した行も 1 行ずつ表示します。 |
| `TypewriterText`（`@celesta.studio/components`） | 線またはブロックのカーソル付きで、文字を一文字ずつ入力して見せます。送りは `useTypewriter` と同じです。カーソルは組版を計測した位置に置くので、比例フォント・日本語・`\n`・`maxWidth` の折り返しでも等幅フォントは要りません。 |
| `useTypewriter(text, options)` | 入力済みの `text`、その `length`、`done`、`caretVisible`。入力中は固定、停止中は点滅します。 |
| `useCountUp(to, options)` | `to` まで数える数値。`from`、`delay`、`durationInFrames`、`easing`、`decimals` で調整します。 |
| `Counter`、`StatCard`（`@celesta.studio/components`） | 数えた数値を整形して描く `<Text>` と、ラベル・数値・補足を載せたカードです。`useCountUp` と同じ既定のタイミングで数え、`locale`（既定 `'en-US'`。実行環境の locale には依存しません）、`decimals`（常に表示）、`prefix` / `suffix`、`formatter` で表示を決めます。`anchorX={1}` にすると、桁が増えても末尾の桁と単位が動きません。 |

```tsx title="タイトルを段階的に表示"
<TextReveal x={160} y={300} lineHeight={200} stagger={6}
  style={{ fontFamily: 'Archivo Black', fontSize: 210,
    fill: { type: 'solid', color: '#ecefe8' } }}>
  {'36 DAYS\nOF CELESTA'}
</TextReveal>
```

等幅フォントのカーソル位置は、`length × advance` を目安に計算できます。実際の文字の位置や幅を使う場合は、デスクトップの useTextMetrics() を利用してください。詳しくは「テキストとフォント」を参照してください。

## カメラ

`<Camera>` は大きなワールドの一部を映します。ワールドを独自の座標で配置し、`x` / `y`（中心に映す点）、`zoom`、`rotation` を動かします。`shake` は指定したピクセル数までの滑らかな手持ち風の揺れを加えます。

```tsx title="長いタイムラインに沿って移動"
<Camera x={interpolate(frame, [0, 120], [0, 4000], { extrapolateRight: 'clamp' })}
  y={540} zoom={1.1} shake={4}>
  <TimelineWorld />
</Camera>
```

速いカメラの動きをぼかすには、カメラを `MotionBlur` で包み、ブラーが渡すフレームから props を計算して、各時点で値が変わるようにします。[アニメーション](/ja/docs/animation/)を参照してください。

## 線とグラフ

`<Line x1 y1 x2 y2>` は線分、`<Polyline points>` は複数の点を結びます。どちらも `stroke`、`strokeWidth`、`cap`（既定 `round`）、`join` を取ります。折れ線の `progress` を 0〜1 に動かすと線が描かれ、`pointOnPolyline(points, t)` で先端にマーカーを置けます。

ほかの形は `<Path>` に `points`（`closed` を指定）または SVG 風の `commands`（`moveTo`、`lineTo`、`quadTo`、`cubicTo`、`close`）、`stroke` や `fill`、`cap` / `join` / `miterLimit` を渡します。区間数によらず 1 レイヤーで、拡大しても鮮明です。半透明の線が交差しても重ね塗りされないので、生成する線画は多数の細い矩形より少数のパスで描いてください。

線や図形の stroke は、線と隙間の長さを交互に並べた `dash`（SVG の `stroke-dasharray` と同じ）と、模様を線に沿ってずらす `dashOffset` を取ります。`dash={[0, 12]}` と `cap="round"` で点線になり、`dashOffset={-frame}` で破線が終点へ向かって流れます。模様は最初の点を基準にするので、`progress` で描いていく破線の折れ線でも、描かれた破線は動きません。閉じた図形の破線は時計回りに並び、円・楕円は右端から、`Rect` は左上の角丸の終わりから始まります。`<Arrow>` は軸だけを破線にします。

`<Path>` も `progress` と `progressStart`（経路長に対する 0〜1）で線の一部だけを描けます。曲線も長さに沿って正確に切り出します。どの部分を描いても、破線は全体の線と同じ位置に並びます。`fill` は常に形全体を塗ります。

`@celesta.studio/components` の `BarChart` は、`{ id, label, value }` の配列から横棒グラフを描きます。行ごとにラベルの列、0 から伸びる棒、数値（棒の先端か右端に揃えた列）が並び、行は `stagger` フレームずつ遅れて始まります。棒の長さと数値は同じ進捗から決まるので、表示される数値は常に棒と一致します。数値は `Counter` と同じ整形です。`Ranking` は順位の列を加えたもので、値でソートするか（`'descending'` / `'ascending'`、同値は入力順を保持）入力順を保つか（`'input'`）を `order` で必ず指定します。値は 0 以上の有限数で、`id` は一意である必要があります。

```tsx title="時間差で伸びる棒グラフ"
<BarChart x={120} y={240} width={1000} stagger={8} decimals={1} suffix=" s" data={[
  { id: 'a', label: 'A', value: 12.4 },
  { id: 'b', label: 'B', value: 8.1 },
]} />
```

`@celesta.studio/components` の `Callout` は対象を指す注釈です。`x` / `y` に置いた幅 `width` の枠に `title` と `text` を入れ、高さは文字を計測して決めます。枠から `target` の点まで線を引き、先端に矢印を付けます。線は対象に面した辺（または `side` で指定した辺）の中点から出て、`straight` ならまっすぐ、`elbow` なら一度だけ曲がって対象に届きます。`lineProgress` を動かすと線が対象側から伸びます。`target`・`x`・`y` は同じ座標系なので、指す対象と同じ `<Group>` に置けばその変形に関係なく線が対象からずれず、対象を動かせば線も追従します。対象が枠の内側にあるときは線を描きません。

```tsx title="矢印付きの注釈"
<Callout target={{ x: 960, y: 540 }} x={1200} y={240} width={480}
  title="ここに注目" text="対象の特徴を説明します"
  fill="#182030" color="#F4F1EA" lineColor="#FF4D1F" />
```

## パスに沿って動かす・形を変える

`commands` を受け取る関数は、コマンドと同じ `{ x, y }` の形で点を返します（`pointOnPolyline` は `points` と同じ `[x, y]`）。

- `pointOnPath(commands, t)` と `tangentOnPath(commands, t)` は、経路の `t`（0〜1）の位置と向きを返します。`pointOnPath(commands, progress)` は `<Path progress>` の先端と一致します。接線の `angle` は時計回りの度数なので、そのまま `rotation` に渡せます。
- `getLength`、`getPointAtLength`、`getTangentAtLength` は同じことをピクセル単位の長さで行います。`measurePath(commands)` は一度測ったものを使い回します。
- `interpolatePath(a, b, t)` は 2 つのパスの中間の形を返します。コマンドの数や種類が違っても、輪郭の長さの割合で角を対応させて補間します。
- `parseSvgPath(d)` は SVG の `d` 文字列（円弧 `A` を含む全コマンド）をコマンドに、`toSvgPath(commands)` はその逆に変換します。

```tsx
const ROUTE = parseSvgPath('M0 0 C 100 -80 200 80 300 0');
const t = progress(frame, 0, 90);
const tip = pointOnPath(ROUTE, t);

<Path commands={ROUTE} stroke="#4DD6FF" strokeWidth={4} progress={t} />
<Path x={tip.x} y={tip.y} rotation={tangentOnPath(ROUTE, t).angle} fill="#FFFFFF"
  points={[[12, 0], [-8, -7], [-8, 7]]} closed />
```

## 円・楕円・矢印

`<Circle radius>` と `<Ellipse width height>` は `Rect` と同じように配置します。`x` / `y` でボックスのアンカー（既定は左上）を置き、`fill` のグラデーションは左上を基準とし、`stroke` はボックス内に収まります。`<Arrow x1 y1 x2 y2>` は先端を `x2` / `y2` に向けます。`strokeWidth`、`headLength`、`headWidth`、`heads`（`end`、`start`、`both`）を指定できます。矢印が短いと頭も縮むので、長さ 0 から伸ばせます。長さ 0 では何も描きません。円は丸い `Rect`、ほかの 2 つはパスで描きます。いずれもグラデーション・変換・効果を使えます。

```tsx title="ラベル付きの注釈"
<Circle x={960} y={540} anchorX={0.5} anchorY={0.5} radius={80}
  stroke="#FFD84D" strokeWidth={8} />
<Arrow x1={600} y1={300} x2={890} y2={480} strokeWidth={6} stroke="#FFD84D" />
```

## 三角形・多角形・星・扇形・ハート

`<Polygon radius sides>`、`<Star radius points innerRatio>`、`<Pie radius startAngle endAngle>` は半径で大きさを決め、同じ半径の `<Circle>` と同じように配置します。`<Triangle width height apex>` と `<Heart width height>` は、`Rect` と同じように配置したボックスいっぱいに描きます。多角形の角と星の先端は 12 時の位置から始まります。扇形の角度は 12 時から時計回りの度数です。`endAngle` を `startAngle` からアニメーションすると扇形が開きます。2 つの角度が等しいと何も描かず、360° 離れると円になります。`cornerRadius` はハート以外の角を丸めます。どの図形も 1 つのパスで、`stroke` は円や楕円と違って輪郭の中心に描きます。`<Path>` と同じく `join` / `miterLimit` も指定できます。輪郭は閉じているので、`cap` が効くのは破線の端だけです。

`trianglePath`、`polygonPath`、`starPath`、`piePath`、`heartPath` は、同じ形のパスコマンドをボックスの左上を原点として返します。そのまま `<Path>` に渡すことも、変形してから渡すこともできます。

```tsx title="開いていく円グラフ"
<Pie x={760} y={340} radius={200} endAngle={interpolate(frame, [0, 45], [0, 270], { extrapolateRight: 'clamp' })}
  fill="#4DD8C0" />
<Star x={1200} y={340} radius={120} points={5} innerRatio={0.45} cornerRadius={8} fill="#FFD84D" />
<Path x={1200} y={640} commands={starPath({ radius: 120, points: 5 })} stroke="#FFFFFF" strokeWidth={4} />
```
