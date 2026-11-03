`@celesta.studio/code` はコンポジション内に構文色付きのソースコードを描きます。[twinkleplop](https://twinkleplop.pngwn.at) でトークン化し、各行を色の区間を持つ Celesta の `Text` 1 つで、行の強調を `Rect` で描くので、ほかのレイヤーと同じく拡縮・フェード・ぼかし・書き出しができます。任意のパッケージで、`@celesta.studio/react` は依存しません。

> [!NOTE]
> **利用できる環境**
> アプリと CLI は `@celesta.studio/code` を同梱し、**File → Set Up TypeScript** で `@celesta.studio/react` と `@celesta.studio/math` と一緒に型宣言を追加できます。npm に公開されていないのでインストール不要です。[ウェブエディタ](/ja/#playground)はまだ対応しないため、インポートする場合はアプリか CLI を使ってください。

## 最初のコードブロック

```tsx title="code-block.tsx"
import { Assets, Composition, Font, Rect } from '@celesta.studio/react';
import { Code, codeThemes } from '@celesta.studio/code';

const source = `const message: string = 'Hello, Celesta.';
console.log(message);`;

const style = { fontFamily: 'IBM Plex Mono', fontSize: 40, lineHeight: 60 };

export default function Root() {
  return (
    <Composition width={1280} height={480} fps={30} durationInFrames={90}>
      <Assets>
        <Font src="./fonts/IBMPlexMono-Regular.ttf" />
      </Assets>
      <Rect width={1280} height={480} fill="#161418" />
      <Code x={64} y={64} language="ts" style={style} theme={codeThemes.dark}>
        {source}
      </Code>
    </Composition>
  );
}
```

`Code` は唯一の子としてソースの文字列を取ります。インデントと末尾の改行も描画の一部なので、文字列内に残してください。

## フォント

**等幅**フォントを使います。`Code` の既定は JetBrains Mono、24 px、行高は文字サイズの 1.5 倍です。上のように `<Font src="…" />` で読み込むか、描画するマシンにインストールします。不足時はほかのテキストと同様にフォールバックします。

## props

**Code の props**

| API | 用途 |
| --- | --- |
| `children` | ソースの文字列。 |
| `language` | `tsx`、`ts`、`json`、`bash`、または `text`（既定、色なし）。 |
| `style` | Celesta の `TextStyle`。`fontFamily`、`fontSize`、`lineHeight` などを指定します。左揃えで折り返さず、色はテーマが設定します。ほかの `align` はエラーです。合字を持つ書体で `===` や `->` を書いたとおりに描くには、`fontFeatures: { liga: false, calt: false }` を指定します。 |
| `theme` | トークンの色、既定の文字色、行の強調色。既定は `codeThemes.dark`。 |
| `tabSize` | タブ幅を文字数で指定します。既定は `2`。 |
| `highlightLines` | 帯で強調する行番号。1 始まりです。 |
| `highlightWidth` | 帯の幅をピクセルで指定します。既定はソースの幅。周囲のパネルの端まで伸ばせます。 |
| `visibleCharacters` | 入力効果で表示するソース量。[code：入力・カーソル・トークン](/ja/docs/code-typing/)を参照。 |
| `x`, `y`, `scale`, `rotation`, `anchorX`/`anchorY`, `opacity`, `blendMode`, `blur`, `shadow`, `glow` | 通常のレイヤーの props はブロック全体に適用されます。 |

## 行の強調

`highlightLines` は指定した行の背後に、幅と行高いっぱいの帯を描きます。コードを入力中でも表示されます。

```tsx title="1 行を強調"
<Code x={64} y={64} language="ts" style={style} highlightLines={[2]} highlightWidth={1152}>
  {source}
</Code>
```

フレームに応じてリストを変えると強調も動きます。各帯は軽量なので、拍やキューごとに行を変えられます。

## テーマ

`codeThemes.dark` と `codeThemes.light` をそのまま使えます。テーマは背景を描かないので、デザインに合う `Rect` を背後に置きます。組み込みテーマを展開し、必要な値を上書きするとカスタマイズできます。

```tsx title="カスタムテーマ"
const theme = {
  ...codeThemes.dark,
  highlightLine: '#a68bbf30',
  tokens: { ...codeThemes.dark.tokens, keyword: '#e0b7ff' },
};
```

トークン名は twinkleplop の `keyword`、`string`、`comment`、`number`、`function`、`type`、`tag_name`、`attr_name`、`punctuation`、`operator` などです。JSON のキーは `property`、文字列値は `string` と `string_escape` です。bash では語の途中のハイフン、ドット、スラッシュもその語の `identifier` に含まれる（`Celesta-export`、`scripts/migrate-packages.mjs`）ので、`keyword` と `builtin` は語全体にだけ付き、`--bit-depth` のようなフラグは 1 つの `parameter` です。テーマにない名前は `foreground` を使います。

[入力効果・カーソル・トークン](/ja/docs/code-typing/)へ進みましょう。
