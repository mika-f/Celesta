## 座標とアンカー

キャンバスの原点は左上で、単位はピクセルです。レイヤーの `x` と `y` は**アンカー**を配置し、既定のアンカーは左上です。`anchorX` と `anchorY` を 0〜1 に設定して移動できます。`0.5` が中心、`1` が右端または下端です。回転（度）と拡縮もアンカーを中心に行います。

```tsx title="中央揃え・回転・拡縮"
<Rect x={960} y={540} anchorX={0.5} anchorY={0.5}
  width={400} height={400} cornerRadius={48}
  rotation={12} scale={0.8} opacity={0.9} fill="#a68bbf" />
```

`scaleX` と `scaleY` は各軸の `scale` を上書きします。`opacity` はグループを通じて乗算されます。1 未満の `Group` は、CSS の `opacity` と同じく子をまとめて一枚の絵として半透明にするため、重なった子どうしが透けて見えることはありません。

## スキューと 3D

すべてのレイヤーと `Group` は、CSS の transform と同じように、傾けたり奥行き方向に回転させたりできます。

| プロパティ | 意味 |
| --- | --- |
| `skewX`, `skewY` | 度単位のせん断で、CSS の `skew()` と同じです。`skewX` は縦の辺を、`skewY` は横の辺を斜めにします。 |
| `rotationX`, `rotationY` | 水平軸・垂直軸まわりの回転（度）で、CSS の `rotateX()`、`rotateY()` と同じです。`rotationY` が正だと右の辺が奥へ、`rotationX` が正だと上の辺が奥へ傾きます。 |
| `perspective` | そのレイヤー自身の 3D 回転に対する視点までの距離（ピクセル）で、CSS の `perspective()` と同じです。小さいほど遠近感が強くなります。指定しない場合（または正の値でない場合）は正射影になり、平行な辺は平行のままです。 |
| `originX`, `originY` | 拡縮・スキュー・各回転の中心で、`perspective` の消失点でもある点を、アンカーから（`Group` では自身の原点から）のピクセルで指定します。既定は 0 です。レイヤー自体は動かさないため、変換がなければ `x` と `y` の位置のままです。 |
| `backfaceVisibility` | `"visible"`（既定）または `"hidden"`。`"hidden"` は、背面が視点側を向いている間そのレイヤー（Group なら中身すべて）を描きません。 |

変換は、レイヤーに近い側から順に `scale`、スキュー、`rotation`、`rotationY`、`rotationX`、`perspective`、最後に `x`/`y` の順で適用されます。`rotation` や `scale` と同じく、すべてアンカーを中心に働きます。`Group` には大きさがないため、自身の原点を中心に回転します。`originX` と `originY` を指定すると、レイヤーを動かさずにこの中心を移せます。`perspective` の消失点もこの中心なので、レイヤーはキャンバスの中央ではなくこの点に向かって遠ざかります。

```tsx title="画面全体の Group をキャンバスの中央を軸に回す"
<Group originX={960} originY={540} rotationY={angle} perspective={2000}>
  {children}
</Group>
```

`perspective` を伴わないスキューや 3D 回転は、レイヤーの形を変えるだけなので、非一様な拡縮と同程度のコストで描けます。`perspective` と 3D 回転の両方を持つレイヤーは、専用の絵に描いてから親へ投影します。描くのはキャンバスに映る部分だけで、画面上で最も拡大される部分に合わせた解像度で作るため、近くの文字やパスは鮮明なままで、遠い側は縮小時にフィルタリングされるのでちらつきません。この絵の大きさは GPU の最大テクスチャサイズと、キャンバスの 4 倍の画素数（ブラウザのエディタではさらに約 1,670 万画素）までに制限されます。何画面分も縦に続くテキストのスクロールのように遠くまで続く平面は、一つの解像度では収まらないため、距離ごとの帯に分けて描かれ、各帯はその帯でいちばん手前の部分に合わせた解像度になります。手前の行は鮮明なまま、遠くの行には必要な分の画素だけが使われ、帯の継ぎ目は見えません。一枚のテクスチャに収まらない大きなテキストレイヤーは、映る部分だけが描かれます。ただし、非常に大きくてもほとんど奥行きのない平面や、中身が `shader` やスタイライズ効果を使う平面は、これまでどおり一つの解像度で描かれ、ぼやけることがあります。専用の絵になるので、この方法で描かれる Group は分離され、CSS と同じく、子の `blendMode` は子どうしの間でのみ混ざります。この描き方になるのは回転している間だけです。3D 回転がちょうど 0 のときは平らで分離されないため、`rotationY={frame * 4}` のように 0 から始まる回転では、2 フレーム目からブレンドする子の混ざり方が変わることがあります。最初から分離しておくには、`rotationY={1 + frame * 4}` のように小さな角度から始めてください。

子は CSS の `transform-style: flat` と同様に、親の平面に平らに畳まれます。回転した Group の子は奥行きで並べ替えられず、書いた順に、Group の平面の上へ描かれます。表と裏のあるカードは、それぞれの面を別の Group にして、背面が向いている側を隠してください。レイヤーが裏を向いているかは、すべての親の回転を重ねて判断します。180 度回転した Group の中の面は、カードが裏返るまで隠れたままです。ミラー（`scaleX={-1}`）では裏向きになりません。ちょうど真横を向いたレイヤーは表向きとして扱われますが、何も描かれません。

```tsx title="裏返るカード"
<Group x={960} y={540} rotationY={frame * 4} perspective={1400}>
  <Group backfaceVisibility="hidden">
    <Rect x={-170} y={-235} width={340} height={470} cornerRadius={28} fill="#f4efe6" />
    <Text anchorX={0.5} anchorY={0.5}
      style={{ fontSize: 64, align: 'center', fill: { type: 'solid', color: '#1d3557' } }}>表</Text>
  </Group>
  <Group rotationY={180} backfaceVisibility="hidden">
    <Rect x={-170} y={-235} width={340} height={470} cornerRadius={28} fill="#1d3557" />
    <Text anchorX={0.5} anchorY={0.5}
      style={{ fontSize: 64, align: 'center', fill: { type: 'solid', color: '#ffffff' } }}>裏</Text>
  </Group>
</Group>
```

`perspective` と 3D 回転を持つレイヤー自身の効果（`blur`、`shadow`、`glow`、色補正やスタイライズのフィルタ）は、他の変換の後と同じく、投影のあとに出力ピクセル単位で適用されます。その内側のレイヤーの効果の大きさも出力ピクセルで測るため、レイヤーが回転してもぼかしの大きさは変わりません。一方、内側のレイヤーのカスタム `shader` や組み込みフィルタは、そのレイヤー自身の絵の上で動くため、ピクセルは絵のピクセルとなり、遠い側では投影に合わせて縮みます。ぼかし、影、グローの半径はどこでも 64 ピクセルが上限ですが、内側では絵のピクセルで数えるため、手前のように絵が出力より細かい所では、大きなぼかしが他の場所より小さく出ます。ポインターの当たり判定は、投影後の形に従います。

CPU リファレンスレンダラはスキュー、`rotationX`、`rotationY` を描画せず、`rotation` と同じく、これらを使うシーンを拒否します。プレビュー、書き出し、ブラウザのエディタでは描画されます。ブラウザのエディタはそれぞれの絵を Canvas 2D で小さな平らな三角形に分けて描くため、キャンバスに広がる床のような大きな遠近のレイヤーでも、1 フレームにかかるのは数十ミリ秒です。ただし、輪郭がプレビューや書き出しより最大で半ピクセルずれたり、遠い側がわずかにぼけたり鋭くなったりすることがあります。プレビューと書き出しでは GPU で投影します。カードの裏返し、傾いたカード、傾けたタイルの床は `examples/with-3d-transform.tsx` を参照してください。

## 図形

`Rect` は矩形を描き、任意の `cornerRadius`、`fill`、内側の `stroke` と `strokeWidth` を指定できます。色は `#RRGGBB` や `#RRGGBBAA` のような、アルファを付けられる 16 進文字列です。正方形の角丸を辺の半分にすると円になります。

`<Instances>` 全体の `fill` にも `#RRGGBB` または `#RRGGBBAA` を指定します。非 ASCII 文字を含む色は、ネイティブのプレビューや書き出しで不正な色としてエラーになります。

## ぼかし・影・光彩

映像のレイヤーや Group に blur、shadow、glow を指定できます。値はフレームごとに変更でき、ぼかしの半径（最大 64 ピクセル）と影のずれは出力画像のピクセル単位です。Group では、子要素をまとめた結果に効果を適用します。

```tsx title="光彩と影の付いたタイトル"
<Group glow={{ color: "#FFCC6680", blur: 12 }}>
  <Text style={{ fontSize: 96 }}
    shadow={{ color: "#000000B0", blur: 6, offsetX: 4, offsetY: 6 }}>
    Celesta
  </Text>
</Group>
```

## 色補正

映像のレイヤーや Group には色補正も指定できます。値はフレームごとに変更できます。`brightness`・`contrast`・`saturation`（1 で元の色）、`hue`（色相を回す角度）、`grayscale`・`invert`（0 から 1）、`tint`（画素に色を乗せる。`{ color, amount }`）、`vignette`（0 から 1、または `{ amount, radius, softness }`。レイヤーの矩形の端に向かって暗くする）があります。どれも CSS の `filter` 関数と同じく、乗算済みでない sRGB の色にかかり、アルファは変えず、結果を 0 から 1 に収めます。`shader` の後に、ここに挙げた順で適用され、blur、shadow、glow より先にかかります。次に挙げるスタイライズのエフェクトは、その前後にかかります。プレビュー、書き出し、ブラウザ版エディタのどれでも動きます。

```tsx title="古い写真のように"
<Image src="street.jpg" grayscale={1} tint={{ color: "#704214", amount: 0.35 }}
  contrast={1.1} vignette={0.6} />
```

別の順番で適用したいときや、カスタムシェーダーの間に挟みたいときは、`@celesta/shader` の同名の関数を `shader` の配列に入れます：`shader={[ripple({ time }), grayscale(), vignette(0.6)]}`。

## スタイライズ

映像のレイヤーや Group には、組み込みのスタイライズのエフェクトも指定できます。値はフレームごとに変更できます。大きさや距離は出力のピクセル、角度は度で指定し、格子や中心はレイヤーの矩形を基準にします。

- `pixelate`：正方形のセルごとに平均します。数値はセルの大きさです。
- `halftone`：セルごとにその色の点を描き、点の面積をセルの明るさに合わせます。点の間は透明になります。数値は格子の間隔で、`{ size, angle }` で格子を回せます（既定は 45 度）。
- `chromaticAberration`：赤と青を逆向きにずらします。数値はずらす量で、`{ offset, angle }` で向きを指定します。
- `wave`：画素を正弦波に沿って揺らします。`{ amplitude, wavelength, phase, angle }` で指定し、`phase` を動かすと波が進みます。
- `noise`：粒子状のノイズを加えます。0 から 1、または `{ amount, seed, monochrome }` で指定します。`seed` を省くと、そのレイヤーの `useCurrentFrame()` に合わせて毎フレーム変わりますが、同じフレームは何度描いても同じになります。止めたいときは `seed` を指定します。
- `scanlines`：横縞を暗くします。0 から 1、または `{ amount, spacing, thickness, offset }` で指定します。
- `zoomBlur`：中心に向かってぼかし、高速なズームのように見せます。0 から 1、または `{ amount, centerX, centerY }` で指定します。

props は `chromaKey`、`pixelate`、`wave`、`zoomBlur`、`chromaticAberration`、色補正、`halftone`、`noise`、`scanlines` の順に、どれも `shader` の後、blur、shadow、glow の前にかかります。別の順番で適用したいときは、`@celesta/shader` の同名の関数を `shader` の配列に入れます。プレビュー、書き出し、ブラウザ版エディタのどれでも動きます。

```tsx title="すり切れたビデオテープ"
<Group chromaticAberration={3} noise={0.12} scanlines={{ amount: 0.4, offset: frame / 4 }}>
  …
</Group>
```

## クロマキー

`chromaKey` はキーの色に近い色を透明にします。グリーンバックで撮った `<Video>` や `<Image>`（ほかのレイヤーでも）を合成するのに使います。文字列はキーの色で、`{ color, tolerance, softness, spill }` で調整します。

- `tolerance`（既定 0.2）：色の色度（BT.709 の CbCr）がキーの色にどれだけ近ければ透明にするか。明るさは見ないので、スクリーンの影の部分も同じように抜けます。
- `softness`（既定 0.1）：`tolerance` からどれだけ離れるまでに不透明へ戻すか。境界がなめらかになります。
- `spill`（既定 0.5、0 から 1）：残った部分から、キーの色かぶりをどれだけ取り除くか。明るさは保ちます。

無彩色はキーの色の色度と同じだけキーから離れています（`#00b140` なら 0.36、`#00ff00` なら 0.60）。`tolerance + softness` をそれより小さくしないと、灰色や黒も半透明になります。値はフレームごとに変更できます。`examples/greenscreen` に、Celesta 自身で描いた素材を抜く作例があります。

```tsx title="グリーンバックを抜く"
<Video src="presenter.mp4" chromaKey={{ color: "#00b140", spill: 0.8 }} />
```

## カスタムシェーダー

`@celesta/shader` の `defineShader` で、WGSL のフィルタを定義できます。`fn effect(input: EffectInput) -> vec4f` を書き、出力の各ピクセルを乗算済みアルファの RGBA で返します。レイヤーの画素は `source_at(position)`（シーンのピクセル座標、補間あり）か `source_load(pixel)` で読みます。レイヤーの矩形の外を `source_at` で読むと透明になります。画面全体を歪ませるシェーダーで端に暗い帯が出るときは、矩形の端で折り返して読む `source_at_mirrored(position)` か、端の画素で止める `source_at_clamped(position)` を使います。`input.uv` はレイヤーの矩形の中での位置（0,0 から 1,1）で、`celesta` にはシーンの大きさとその矩形が入っています。パラメータは `params` で宣言し、使うたびに値を渡します。時間もパラメータとして渡すので、シーケンスや FreezeFrame の中でも正しく動きます。結果はどのレイヤーの `shader` にも渡せます。`shader={[ripple({ time }), rgbSplit({ amount: 6 })]}` のように配列を渡すと、複数のシェーダーを順に適用し、それぞれが前のシェーダーの結果にかかります。どのシェーダーも同じレイヤーの矩形を見ており、パディングは足し合わされます。シェーダーは色補正の props、blur、shadow、glow より先に適用され、レイヤーの矩形と `padding` の範囲にだけ書き込みます。ソースは `.wgsl` ファイルとして import でき、エディタのハイライトが効くうえ、保存するとプレビューが再読み込みされます。シェーダーはプレビューと書き出しで動き、ブラウザ版エディタでは使えません。

```tsx title="波打つタイトル"
import { defineShader } from '@celesta/shader';

const ripple = defineShader({
  name: 'ripple',
  wgsl: `fn effect(input: EffectInput) -> vec4f {
    let center = mix(celesta.content.xy, celesta.content.zw, vec2f(0.5));
    let offset = input.position - center;
    let wave = sin(length(offset) * 0.06 - params.time * 6.0) * params.amplitude;
    let direction = offset / max(length(offset), 1e-4);
    return source_at(input.position - direction * wave);
  }`,
  params: { time: 'f32', amplitude: { type: 'f32', default: 6 } },
  padding: 8,
});

<Text style={{ fontSize: 120 }} shader={ripple({ time: frame / fps })}>
  Celesta
</Text>
```

## ブレンドモード

各レイヤーの `blendMode` で、CSS の `mix-blend-mode` と同様に下の描画との混ぜ方を変えます。`normal`（既定）、`multiply`、`screen`、`overlay`、`add`、`difference`、`darken`、`lighten`、`colorDodge`、`colorBurn`、`hardLight`、`softLight`、`exclusion`、`hue`、`saturation`、`color`、`luminosity` が使えます。計算は W3C Compositing and Blending の式どおりで、2 語の名前は CSS の `color-dodge` ではなく `colorDodge` のように書きます。`Group` で `normal` 以外を選ぶと、先に子をまとめて描き、その結果を 1 レイヤーとして混ぜます。

```tsx title="明るい背景でも暗い背景でも読める HUD"
<Group blendMode="difference" opacity={0.8}>
  <Text x={48} y={48} style={{ fill: { type: 'solid', color: '#ffffff' } }}>
    REC 00:00:12
  </Text>
</Group>
```

## グループ化と配置

`Group` 自体はサイズを持ちません。子は `x` / `y` を基準に配置され、回転・拡縮・透明度が全体に適用されます。レイアウトのヘルパーもこれを基礎にしています。

**レイアウトのヘルパー**

| API | 用途 |
| --- | --- |
| `Center` | 原点をキャンバスまたは親 `SafeArea` の中心に移動します。 |
| `SafeArea` | `padding`（数値、`{ top, right, bottom, left }`、またはプリセット `'vertical-social'`）で余白を取り、内側のヘルパーが見る領域を縮めます。 |
| `Stack` | 子を `spacing` ピクセル間隔で並べます。既定は `vertical`、`horizontal` も選べます。 |
| `Grid` | `columns` 列、`columnWidth` × `rowHeight` のセルに子を配置します。間隔も指定できます。 |
| `Fit` | `sourceWidth` × `sourceHeight` で設計した内容を、現在の領域に `contain` または `cover` で合わせます。 |
| `LayoutBounds` | 内側のヘルパーが見る領域を `width` × `height` にします。子を動かさず、何も描きません。 |

```tsx title="セーフマージン内の中央揃えリスト"
<SafeArea padding={96}>
  <Center>
    <Stack spacing={96} y={-96}>
      {['Write', 'Preview', 'Export'].map((step) => (
        <Text key={step} anchorX={0.5} anchorY={0.5}
          style={{ fontSize: 64, fill: { type: 'solid', color: '#332f3b' } }}>
          {step}
        </Text>
      ))}
    </Stack>
  </Center>
</SafeArea>
```

ヘルパーは子の原点を配置し、描画サイズは計測しません。配置点を中心にするには、子のアンカーを `0.5` にします。

## パネル

`@celesta/components` の `Panel` は、`width` × `height` の枠に背景（`fill`）、枠線（`stroke`・`strokeWidth`。枠の内側に描きます）、角丸（`cornerRadius`）、背景が落とす影（`shadow`）をまとめて描きます。子は `padding`（数値または `{ top, right, bottom, left }`）の内側から配置され、内側の `Center`・`Fit`・`useLayoutBounds()` は余白を除いた領域を参照します。描く順序は影、背景、子、枠線で、内容が枠線を覆うことはありません。`clip` を付けると子を角丸の枠で切り取ります。`anchorX`・`anchorY` は `x`・`y` に置く点で、`rotation` と `scale` もこの点を中心にします。寸法・余白・角丸・枠線の太さは有限の 0 以上でなければなりません。

```tsx title="説明カード"
<Panel x={120} y={600} width={720} height={320} padding={32}
  fill="#182030" stroke="#2C3A50" cornerRadius={20} clip
  shadow={{ color: "#00000099", blur: 32, offsetX: 0, offsetY: 16 }}>
  <Text maxWidth={656} style={{ fontSize: 40 }}>説明文</Text>
</Panel>
```

`@celesta/components` の `Badge` は、文字を実際に計測した寸法の `Panel` に載せたラベルです。日本語・英数字・比例フォントのどれでも背景が文字に合い、文字が変わればその場で背景も変わります。文字の `style`（または `fontFamily`・`fontSize`・`fontWeight`・`color` の省略形）、`Panel` の `fill`・`stroke`・`shadow`・アンカー、`padding`（既定は上下 8・左右 16）、`cornerRadius`（既定は 8。`"pill"` で両端が半円になります。チップ用の別コンポーネントはありません）を受け取ります。`leading` は文字の前に描く装飾（ドットなど）で、幅 `leadingWidth`・1 行分の高さの領域に配置され、文字との間は `gap` です。`maxWidth` を指定すると長い文字は折り返し、バッジは縦に伸びます。

```tsx title="章ラベルとドット付きのチップ"
<Badge x={120} y={80} fill="#204060" fontSize={28} color="#FFFFFF">第1章</Badge>
<Badge x={120} y={160} fill="#10121A" cornerRadius="pill" fontSize={22} color="#5BC0FF"
  leading={<Center><Rect anchorX={0.5} anchorY={0.5} width={8} height={8} cornerRadius={4} fill="#3D5BFF" /></Center>}
  leadingWidth={8} gap={12}>
  {"<Composition fps={30}>"}
</Badge>
```

## Shorts・TikTok・Reels 向けの縦型動画

キャンバスは 1080×1920 にします。プレーヤーは動画の上に自分の UI を重ねます。キャプション、チャンネル名、再生バーが下の約 20% を覆い、いいね・コメント・共有のボタンが右端に並びます。見出し、字幕、ロゴ、呼びかけなど読ませるものは `<SafeArea>` の内側に置き、背景、映像、装飾は端まで使います。プリセット `'vertical-social'` は、キャンバスの上 10%、右 18.5%、下 20%、左 1/15 を避けます（1080×1920 では上 192、右 200、下 384、左 72 px）。目安であり、各プラットフォームの仕様ではなく、UI の変更にも追従しません。より正確な値が分かるときは `{ top, right, bottom, left }` で指定します。

```tsx title="文字はプラットフォームのセーフエリア内、背景は端まで"
<Composition width={1080} height={1920} fps={30} durationInFrames={300}>
  <Image src="./bg.jpg" width={1080} height={1920} />
  <SafeArea padding="vertical-social">
    <Center>
      <Text anchorX={0.5} anchorY={0.5} maxWidth={760}
        style={{ fontSize: 96, align: 'center' }}>
        Readable copy
      </Text>
    </Center>
  </SafeArea>
  <DebugOverlay safeArea="vertical-social" />
</Composition>
```

確かめるには、`<DebugOverlay>` に同じセーフエリアを渡し、`--debug-guides` を付けて PNG フレームかコンタクトシートを書き出します（[書き出し](/ja/docs/export/)を参照）。UI が重なる範囲が網掛けされるので、文字が動いている間も含めて、すべて枠の内側に収まっているかを見ます。

## グループを切り抜く

`Group` に `clip` を指定すると、グループの座標系にある矩形 `{ x, y, width, height, cornerRadius }` の内側にだけ子を描きます。マスクによる表示、ワイプ、パネル内のスクロールに使えます。クリップはグループとともに移動・拡縮・回転し、境界はアンチエイリアスされます。入れ子のクリップは共通部分だけを表示します。

```tsx title="境界の裏からテキストをスライド"
<Group x={120} y={200} clip={{ width: 720, height: 96 }}>
  <Text y={96 * (1 - reveal)}
    style={{ fontSize: 88, fill: { type: 'solid', color: '#332f3b' } }}>
    Layer it.
  </Text>
</Group>
```

## グループをマスクする

`Group` に `mask` を指定すると、別に描いたもの（テキスト、パス、画像、部分木全体）がある場所にだけ子を表示します。マスクはグループの座標系に置かれ、それ自体は表示されません。`<Mask mode="luminance" invert>` で包むと、alpha の代わりに明るさで表示したり、マスクのない場所に表示したりできます。マスクしたグループは分離されるため、子の描画モードは子どうしでだけ混ざり、ぼかし・影・光彩は切り抜いた形に付きます。マスクはプレビュー、書き出し、ブラウザのエディタで同じように描かれます。

```tsx title="文字の中のストライプ"
<Group mask={
  <Text x={960} y={540} anchorX={0.5} anchorY={0.5}
    style={{ fontSize: 360, fontWeight: 900 }}>
    CELESTA
  </Text>
}>
  <Stripes />
</Group>
```
