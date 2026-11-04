`@celesta.studio/math` は動きの数値を扱う小さな依存なしのツールキットです。シード付き乱数、滑らかなノイズ、数値の整形、波、角度、2D 座標を提供します。ランタイムに同梱され、直接インポートできます。

```tsx title="インポート"
import { random, randomRange, noise } from '@celesta.studio/math';
```

**File → Set Up TypeScript** でエディタの型を設定します（[最初の React コンポジション](/ja/docs/react-compositions/)参照）。[ウェブエディタ](/ja/#playground)でも `@celesta.studio/react`、`@celesta.studio/math`、`react` を使えます。このパッケージはまだ npm に公開されていません。

この章は乱数を扱います。[ノイズと fbm](/ja/docs/math-noise/)では滑らかな揺れ、[整形・波・幾何](/ja/docs/math-shaping/)では日常的なヘルパーを説明します。

## Math.random() を使わない理由

コンポジションはフレームの関数です。プレビューは逆方向にも移動し、書き出しは順番に進みますが、同じフレームでは同じ絵が必要です。`Math.random()` は呼ぶたびに値が変わるので、スクラブごとに星が飛び回ります。`@celesta.studio/math` の全関数は入力だけで決まる純粋関数で、同じシードならプレビュー・書き出し・各マシンで同じ値を返します。

## シード

`Seed` は数値または文字列です。`random(seed)` は `[0, 1)` の値を返し、同じシードなら同じ値になります。ランダム化する各対象にシードを与え、インデックスやプロパティごとに変えます。

```tsx title="プロパティごとのシード"
const x = randomRange(`star-${i}-x`, 0, 1920);
const y = randomRange(`star-${i}-y`, 0, 1080);
const size = randomRange(`star-${i}-size`, 2, 6);
```

- シードを再利用すると値も再利用されます。`random('a')` は 2 回呼んでも同じ値なので、`x` と `y` に別のシードを使わないと星が対角線に並びます。
- 小数にも独自のシードがあります。`0.5` は `0` や `1` と衝突しません。一方、`0.5` と `'0.5'` のように数値とその文字列表現は同じシードです。
- シードは文字列または有限の数値にしてください。`NaN` と `Infinity` は関数名を含むエラーを投げます。
- 時間で値を変えるにはシードに時間を入れます。`random(Math.floor(frame / 4))` は 4 フレームごとに変わり、ちらつきやグリッチに向きます。滑らかに変えるには [`noise`](/ja/docs/math-noise/) を使います。

## 乱数の関数

**乱数**

| API | 用途 |
| --- | --- |
| `random(seed)` | `[0, 1)` の数値。 |
| `randomRange(seed, min, max)` | `[min, max)` の数値。 |
| `randomInt(seed, min, max)` | `min`〜`max` の整数で、**両端を含む**。整数がなければエラーになります。 |
| `randomBool(seed, probability?)` | 指定した確率で `true`。既定は `0.5`。 |
| `randomSign(seed)` | `-1` と `1` を半々の確率で返します。方向や回転の反転に便利です。 |
| `randomPick(seed, items)` | `items` の要素をひとつ返します。空リストはエラーです。 |
| `shuffle(seed, items)` | `items` を並べ替えた**コピー**を返し、元のリストは変更しません。 |
| `randomGaussian(seed, mean?, stdDev?)` | 正規分布の値。多くは `mean` の周囲 `stdDev` 内に入り（既定 `0` と `1`）、一部は遠く離れます。 |
| `randomInCircle(seed, radius?, center?)` | 円盤の面積に均等に散らばる `{ x, y }` の点。既定は原点中心、半径 `1`。 |

## シード付きの星空

シードが固定なので、星の配置は全フレームで同じです。[`noise`](/ja/docs/math-noise/) による移動だけが変わります。

```tsx title="ゆっくり動く星空"
import { noise, randomRange } from '@celesta.studio/math';

function Stars() {
  const frame = useCurrentFrame();
  return <>
    {Array.from({ length: 80 }, (_, i) => (
      <Rect key={i} width={4} height={4} cornerRadius={2} fill="#ffffff"
        x={randomRange(`star-${i}-x`, 0, 1920) + noise(i, frame / 40) * 30}
        y={randomRange(`star-${i}-y`, 0, 1080)}
        opacity={randomRange(`star-${i}-o`, 0.2, 0.8)} />
    ))}
  </>;
}
```

## 選択と並べ替え

`randomPick` と `shuffle` はパレットやキャプションなどの既存リストから選びます。インデックスなど安定した値をシードにします。

```tsx title="パレットとランダムな順序"
const palette = ['#a68bbf', '#7cf29c', '#f2c97c', '#7cc4f2'];

const color = randomPick(`bar-${i}-color`, palette);
const order = shuffle('slides', ['intro', 'demo', 'pricing', 'outro']);
const mirrored = randomBool(`bar-${i}-flip`, 0.3); // flipped about 30% of the time
const spin = 45 * randomSign(`bar-${i}-spin`);     // clockwise or counter-clockwise
```

## 自然な散らばり

一様乱数は全値の確率が等しく、人工的に見えることがあります。`randomGaussian` は中心付近に値を集め、`randomInCircle` は円盤の中央に偏らず面積に均等に点を散らします。粒子の広がりが中心に密集した星型ではなく、丸くなります。

```tsx title="粒子の広がり"
import { randomGaussian, randomInCircle } from '@celesta.studio/math';

const particles = Array.from({ length: 120 }, (_, i) => {
  const p = randomInCircle(`spark-${i}`, 260, { x: 960, y: 540 });
  const size = Math.max(2, randomGaussian(`spark-${i}-size`, 8, 3));
  return <Rect key={i} x={p.x} y={p.y} anchorX={0.5} anchorY={0.5}
    width={size} height={size} cornerRadius={size / 2} fill="#f2c97c" />;
});
```

> [!NOTE]
> **シードを変えて見た目を調整**
> 配置がほぼ気に入ったときは、シードの接頭辞を `star-` から `sky-` に変えると全体が並び替わります。値は再現可能なので、プレビューで選んだ見た目がそのまま書き出されます。
