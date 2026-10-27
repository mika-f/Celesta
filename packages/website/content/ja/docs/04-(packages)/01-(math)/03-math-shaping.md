[`@celesta/math`](/ja/docs/math/) の残りのヘルパーは、数値を整形し、繰り返す動きを作り、配置や軌道に必要な角度・点の計算をします。通常の関数なので、コンポーネント、`prepare()`、データファイルなど、どこでも使えます。

## 数値の整形

**スカラー**

| API | 用途 |
| --- | --- |
| `clamp(value, min, max)` | `value` を `[min, max]` に制限します。`clamp01(value)` は `[0, 1]` に制限します。 |
| `lerp(a, b, t)` | `a` から `b` へ `t` 進んだ値。`t` は制限されず、0〜1 の外では外挿します。 |
| `inverseLerp(a, b, value)` | 逆に、`a`〜`b` に対する `value` の位置を `t` で返します。`a === b` の場合は `0`。 |
| `remap(value, inMin, inMax, outMin, outMax)` | 制限なしで範囲を変換します。`remapClamped` を使うと出力範囲に収まります。 |
| `step(edge, x)` | `edge` 未満は `0`、`edge` 以上は `1`。 |
| `smoothstep(edge0, edge1, x)` | `x` が `edge0`〜`edge1` を進む間の滑らかな 0〜1。`smootherstep` は開始・終了がさらに緩やかです。 |
| `fract(x)` | 小数部分。負数も含め常に `[0, 1)`。 |
| `mod(value, divisor)` | 除数と同じ符号の剰余。`mod(-1, 4)` は `-1` ではなく `3`。 |
| `wrap(value, min, max)` | `[min, max)` に循環させます。位置や色相のループに。 |
| `pingPong(value, length)` | 増え続ける値を `0` と `length` の間で往復させます。 |
| `snap(value, increment)` | `increment` の最も近い倍数に丸めます。段階的な動きやグリッド配置に。 |
| `roundTo(value, decimals?)` | 小数点以下の桁数を指定して丸め、画面の数値を整えます。 |
| `approxEqual(a, b, epsilon?)` | 2 数の差が `epsilon` 以下か判定します。既定は `1e-6`。 |

> [!NOTE]
> **remap と interpolate の使い分け**
> `remap` はイージングなしの線形変換で、`remapClamped` を使わなければ制限もありません。フレームに沿う動きやイージング、`extrapolateLeft` / `extrapolateRight` には `@celesta/react` の [`interpolate`](/ja/docs/animation/) を使います。`remap` はノイズを透明度に変えるなど、別の種類の値の変換に向きます。

```tsx title="ループと往復"
import { pingPong, snap, wrap } from '@celesta/math';

const frame = useCurrentFrame();
const ticker = wrap(frame * 4, -200, 1920);   // scrolls right, re-enters at the left
const sweep = pingPong(frame * 8, 1600);      // 0 → 1600 → 0, repeating
const stepped = snap(sweep, 100);             // the same sweep, in 100 px jumps
```

## 波

4 つの波関数は**周期 1**で繰り返し、`[-1, 1]` を返すので `frame / framesPerCycle` を渡します。位相が同じで、前半は正、後半は負です。互いに置き換えられます。

**波**

| API | 用途 |
| --- | --- |
| `sineWave(t)` | 滑らかな正弦波。穏やかな脈動・上下動・呼吸。 |
| `triangleWave(t)` | 直線的な上昇・下降。一定速度の往復。 |
| `squareWave(t)` | 周期の前半は `1`、後半は `-1`。オン・オフの点滅。 |
| `sawtoothWave(t)` | 0 から 1 へ上昇し、中間で -1 に落ち、0 に戻ります。繰り返す走査に。 |

```tsx title="2 秒周期で呼吸する円"
import { sineWave } from '@celesta/math';

const { fps } = useVideoConfig();
const frame = useCurrentFrame();
const breath = sineWave(frame / (fps * 2)); // one full cycle every two seconds

<Rect x={960} y={540} anchorX={0.5} anchorY={0.5} width={200} height={200}
  cornerRadius={100} scale={1 + 0.08 * breath} fill="#a68bbf" />
```

## 角度

角度を取る・返す関数は**ラジアン**を使います。レイヤーの `rotation` は**度**なので、計算した角度は `radToDeg` で変換します。

**角度**

| API | 用途 |
| --- | --- |
| `TAU` | ラジアンの 1 回転（2π）。 |
| `degToRad(degrees)`, `radToDeg(radians)` | 度とラジアンの相互変換。 |
| `normalizeAngle(angle)` | 角度を `[-π, π)` に循環させます。 |
| `angleDifference(from, to)` | 2 つの角度の間の最短の符号付き回転量。 |
| `lerpAngle(a, b, t)` | 最短方向で補間します。350° から 10° は 180° を通って戻らず、0° を通ります。 |

## 点と幾何

点は `Vec2` 型の通常の `{ x, y }` オブジェクトです。Celesta の y 軸は下向きなので、正の角度は画面上で `rotation` と同じ**時計回り**になります。

**2D の点**

| API | 用途 |
| --- | --- |
| `distance(a, b)` | 2 点の直線距離。 |
| `angleBetween(from, to)` | 2 点間の方向をラジアンで返します。`0` は +x 方向。 |
| `lerpPoint(a, b, t)`, `midpoint(a, b)` | 線分上の点と中点。 |
| `rotatePoint(point, angle, origin?)` | 指定した原点を中心に回転します。既定は座標原点。 |
| `polarToCartesian(angle, radius, center?)`, `cartesianToPolar(point, center?)` | 方向・距離と `{ x, y }` を相互変換します。後者は `{ angle, radius }` を返します。 |
| `quadraticBezierPoint(p0, p1, p2, t)`, `cubicBezierPoint(p0, p1, p2, p3, t)` | パスの `quadTo` / `cubicTo` と同じベジェ曲線上の `t`（0〜1）の点。描いた曲線に沿って対象を動かせます。 |

```tsx title="回転する輪の上の点"
import { TAU, polarToCartesian } from '@celesta/math';

const frame = useCurrentFrame();
const dots = Array.from({ length: 12 }, (_, i) => {
  const angle = (i / 12) * TAU + frame / 60; // evenly spaced, slowly turning
  const p = polarToCartesian(angle, 300, { x: 960, y: 540 });
  return <Rect key={i} x={p.x} y={p.y} anchorX={0.5} anchorY={0.5}
    width={24} height={24} cornerRadius={12} fill="#a68bbf" />;
});
```

```tsx title="曲線に沿って動くマーカー"
import { cubicBezierPoint } from '@celesta/math';

const p0 = { x: 200, y: 800 }, p1 = { x: 500, y: 200 };
const p2 = { x: 1400, y: 200 }, p3 = { x: 1700, y: 800 };
const t = progress(useCurrentFrame(), 0, 90, Easings.easeInOutCubic);
const marker = cubicBezierPoint(p0, p1, p2, p3, t);

<Path commands={[
  { type: 'moveTo', ...p0 },
  { type: 'cubicTo', x1: p1.x, y1: p1.y, x2: p2.x, y2: p2.y, x: p3.x, y: p3.y },
]} stroke="#57456c" strokeWidth={4} />
<Rect x={marker.x} y={marker.y} anchorX={0.5} anchorY={0.5} width={32} height={32}
  cornerRadius={16} fill="#a68bbf" />
```

`quadraticBezierPoint` と `cubicBezierPoint` は `Path` の `quadTo` / `cubicTo` と同じ制御点を使うので、マーカーが描いた曲線上に留まります。`Path` は[テキスト効果・カメラ・線](/ja/docs/text-camera-lines/)をご覧ください。

## 色

色の props は `#RRGGBB` か `#RRGGBBAA`（大文字・小文字どちらも可）を受け付けます。次の 2 つのヘルパーはこの形式をレンダラーと同じ規則で読むので、データやプロパティ入力から来た色を使う前に確かめたり、チャンネルに分けたりできます。

**色**

| API | 用途 |
| --- | --- |
| `isHexColor(value)` | `value` が `#RRGGBB` か `#RRGGBBAA` の文字列かどうか。 |
| `parseHexColor(color)` | 色を `[r, g, b, a]`（それぞれ 0–255。`#RRGGBB` のアルファは 255）で返します。どちらの形式でもなければ `undefined` です。 |

```tsx title="フォールバック付きのデータの色を、アルファ半分で"
import { isHexColor, parseHexColor } from '@celesta/math';

const fill = isHexColor(row.color) ? row.color : '#a68bbf';
const [r, g, b, a] = parseHexColor(fill)!;
const overlay = `#${[r, g, b, Math.round(a / 2)].map((c) => c.toString(16).padStart(2, '0')).join('')}`;
```
