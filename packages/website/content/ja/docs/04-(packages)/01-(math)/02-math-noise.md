ノイズは滑らかな乱数です。近い入力は近い出力になるので、跳ねずに揺れます。漂い、手持ちの揺れ、浮かぶ粒子、有機的な質感に使えます。[`@celesta.studio/math`](/ja/docs/math/) のほかの関数と同じく、シードと座標が同じなら毎回同じ値です。

## 時間に沿った 1D ノイズ

`noise(seed, t)` は `[-1, 1]` の値を返します。`t` の各整数で乱数を選び、その間を滑らかに補間します。`t` の尺度で速度が決まり、`frame / 20` は約 20 フレームごとに方向を変えます。

```tsx title="ゆったり浮かぶ動き"
import { noise } from '@celesta.studio/math';

function Floating({ children }: { children: React.ReactNode }) {
  const frame = useCurrentFrame();
  return (
    <Group x={noise('float-x', frame / 45) * 24} y={noise('float-y', frame / 60) * 16}
      rotation={noise('float-r', frame / 90) * 3}>
      {children}
    </Group>
  );
}
```

- **各プロパティで別のシード**を使います。同じだと `x` と `y` が連動し、対角線上を動きます。
- 除数が大きいほど遅く穏やかです。`frame / 90` は漂い、`frame / 8` は震えます。
- 必要な振幅を掛けます。`noise(…) * 24` は両方向に最大 24 ピクセル動きます。
- 多数の対象には個別のオフセットを加えます。``noise(`dot-${i}`, frame / 40)`` で各点が別の軌跡になります。

> [!NOTE]
> **カメラの揺れ**
> [`Camera`](/ja/docs/text-camera-lines/) の `shake` は手持ち風の揺れ向けです。カメラ以外を揺らしたり、自分で揺れを整形する場合は `noise` を直接使います。

## 2D と 3D のノイズ

`noise2D(seed, x, y)` は平面上の滑らかな場を取得します。`x / 200` のように座標を縮めると模様が伸びます。除数が小さいほど細かく変化します。`noise3D(seed, x, y, z)` の 3 つ目の座標を時間として使うと、場を滑らかに動かせます。

```tsx title="呼吸する点の場"
import { noise3D } from '@celesta.studio/math';

function Field() {
  const frame = useCurrentFrame();
  const dots = [];
  for (let row = 0; row < 12; row++) {
    for (let col = 0; col < 20; col++) {
      // Two coordinates for position, the third for time.
      const n = noise3D('field', col / 6, row / 6, frame / 60);
      const size = 28 + n * 20;
      dots.push(<Rect key={`${row}-${col}`} x={140 + col * 82} y={100 + row * 80}
        anchorX={0.5} anchorY={0.5} width={size} height={size} cornerRadius={size / 2}
        opacity={0.5 + n * 0.5} fill="#a68bbf" />);
    }
  }
  return <>{dots}</>;
}
```

結果はすべて `[-1, 1]` です。サイズや透明度などに変えるには、[`remap`](/ja/docs/math-shaping/) または上の計算を使います。`0.5 + n * 0.5` は `[0, 1]` に変換します。

## ノイズを重ねる（fbm）

1 層のノイズは単一の尺度で動くので、滑らかでも少し人工的です。フラクタルブラウン運動 `fbm` は、より細かく弱い層を重ねます。雲・煙・地形・炎のように、大きく変化しながら細部も持ちます。`fbm`、`fbm2D`、`fbm3D` は通常版の引数に設定オブジェクトを加え、同じく `[-1, 1]` の値を返します。

**fbm の設定**

| API | 用途 |
| --- | --- |
| `octaves` | 重ねる層数。1 以上の整数で、既定は `4`。増やすと細部が増え、呼び出しの負荷も少し増えます。 |
| `lacunarity` | 次の層の細かさ。既定の `2` は周波数を 2 倍にします。 |
| `gain` | 次の層の弱さ。0 以上の有限数で、既定は `0.5`。小さいほど滑らか、大きいほど粗くなります。 |

```tsx title="凹凸のある地平線"
import { fbm } from '@celesta.studio/math';

const points: [number, number][] = Array.from({ length: 97 }, (_, i) => [
  i * 20,
  700 + fbm('ridge', i / 14, { octaves: 5, gain: 0.55 }) * 120,
]);

<Polyline points={points} stroke="#a68bbf" strokeWidth={3} />
```

無効な設定は `fbm2D() requires a whole number of octaves of at least 1` のような関数名を含むエラーを投げます。

## ループするノイズ

`noise` と `fbm` は繰り返しません。繰り返し再生される動画（Shorts や TikTok）でこれらを使って揺らすと、最後のフレームから最初のフレームへ戻るところで値が飛びます。`loopNoise(seed, t, period)` は同じく滑らかなノイズで、`t` と同じ単位で数えた `period` ごとに繰り返します。ループの長さをフレームと同じ尺度にして渡します。重ねたノイズには `loopFbm` を使います。

```tsx title="ループする揺れ"
import { loopNoise } from '@celesta.studio/math';

const LOOP = 384; // the composition's durationInFrames

function Sway({ children }: { children: React.ReactNode }) {
  const frame = useCurrentFrame();
  return <Group x={loopNoise('sway', frame / 20, LOOP / 20) * 24}>{children}</Group>;
}
```

## どれを使う？

**ノイズ関数の選び方**

| 目的 | 使用する関数 |
| --- | --- |
| 時間とともに揺れる値 | `noise(seed, frame / n)` |
| 複数の尺度で変化する値 | `fbm(seed, frame / n)` |
| ループ動画の中で揺れる値 | `loopNoise(seed, frame / n, durationInFrames / n)` |
| 画面全体の模様 | 縮小した座標を `noise2D` または `fbm2D` に渡す |
| 動く画面全体の模様 | 時間を 3 つ目の座標にした `noise3D` または `fbm3D` |
| フレームや拍ごとの独立した新しい値 | シードにフレームを入れた [`random`](/ja/docs/math/) |
