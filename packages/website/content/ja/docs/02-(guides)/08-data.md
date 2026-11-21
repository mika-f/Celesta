## 描画前にデータを読み込む

各フレームは同期的に描画するため、コンポーネントでファイルや通信を待てません。代わりに `async prepare()` エクスポートを使います。プレビューも書き出しも最初のフレーム前に一度だけ実行されます。結果をモジュールの変数に保存し、描画中に読み取ります。

```tsx title="一度取得して全フレームで使う"
let headlines: string[] = ['Hello from Celesta'];

export async function prepare() {
  try {
    const response = await fetch('https://example.com/headlines.json');
    headlines = (await response.json()) as string[];
  } catch (error) {
    console.error('Using the fallback headline', error);
  }
}
```

オフラインでも描画できるよう既定値を用意します。同じ理由で `loadLipSync()`、`loadPsdPreset()`、`preloadMedia()` も `prepare()` 内で実行します。

## データと props でコンポーネントを再利用する

設定は通常のオブジェクトや読み込んだ JSON データに置き、React コンポーネントの props として渡します。データ用の JSON は、.celesta.json のタイムライン形式とは別のものです。props に型を付けると、エディタで使い方を確認できます。

```tsx file=examples/data.tsx title="data.tsx"
```

> [!NOTE]
> **既存の JSON プロジェクトとの連携**
> ProjectProvider、useProjectProperty()、defineProjectProperties()、registerComponent() は既存の JSON プロジェクトとの連携に使えます。新しい React コンポジションでは必須ではありません。既存ファイルの保守には[旧形式のタイムラインの説明](/ja/docs/timelines/)を参照してください。

## コードを変えずにパレットを切り替える

パレットは、フィルムの色を 4 つの役割に分けます。`ground` はいちばん後ろに敷く地の色、`main` は主役の色、`second` は文字の色、`accent` はところどころに差す色です。少し明るいカードや半透明の線のように役割から作る色は `derived` に一度だけ名前を付けておけば、パレットごとにそのパレットの色から作られます。役割と派生色の名前だけで描いておけば、パレットを替えるだけでフィルム全体の色が変わります。

```tsx title="2 つのパレットをプロパティパネルで選ぶ"
import {
  Composition, Rect, Text, Theme, alpha, defineProjectProperties, definePalettes, getProjectProperty, mix,
} from '@celesta.studio/react';

const palettes = definePalettes({
  harbor: { ground: '#0F2A3D', main: '#5FD3B5', second: '#EAF4F2', accent: '#F2B544' },
  paper:  { ground: '#F4F1EA', main: '#1F4E79', second: '#111111', accent: '#D93A12' },
}, {
  derived: {
    card: mix('ground', 'main', 0.14), // ground 86%、main 14%
    veil: alpha('second', 0.2),        // second を不透明度 20% で
  },
});

defineProjectProperties({ palette: palettes.property({ label: 'Palette' }) });

export default function Film() {
  return (
    <Composition width={1920} height={1080} fps={30} durationInFrames={90}>
      <Theme palette={palettes.get(getProjectProperty('palette'))}>
        <Rect width={1920} height={1080} fill="ground" />
        <Rect x={160} y={160} width={1600} height={760} fill="card" stroke="veil" strokeWidth={2} />
        <Text x={240} y={240} fontSize={96} fontWeight={700} color="second">Harbor report</Text>
        <Rect x={240} y={380} width={320} height={12} fill="accent" />
      </Theme>
    </Composition>
  );
}
```

- `<Theme palette>` は、役割と派生色を `<Theme>` の色名として使えるようにします。`<Text color>`、図形の `fill` と `stroke`、`@celesta.studio/components` の色の props など、`<Theme>` の色名が使えるところならどこでも使えます。一緒に渡した `colors` はパレットの色より優先され、内側の `<Theme>` はこれまでどおり名前を足します。いま効いているパレットは `useTheme().palette` で取れます。
- `mix(a, b, amount)` は `a` から `b` へ `amount` だけ寄せた色で、CSS の `color-mix(in srgb, …)` と同じです。`alpha(color, amount)` は色を変えずに不透明度を `amount` 倍にします。どちらも、役割、それより上で定義した派生色、hex の色を受け取ります。
- `palettes.property()` はパレット名を選択肢にしたメニューのプロパティです。プロパティパネル、エントリと同じフォルダーの `props.json`、エクスポーターの `--props '{"palette":"paper"}'` や `--props-file` でパレットを選べます。パレットにない名前は最初のフレームより前に弾かれます。
- 役割はどれも `#RRGGBB` か `#RRGGBBAA` の色にします。そうでなければ `definePalettes()` が例外を投げます。`ground` に対する `second` のコントラストが文字に足りない 4.5:1 未満のときは、パレット名と比率を書いた警告を出します。同じ指摘は `palettes.check()` でも受け取れます。
