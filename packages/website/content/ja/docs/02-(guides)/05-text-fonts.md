## テキスト

`Text` の子は文字列、数値、`<Span>` 要素です。`style` で装飾します。

```tsx title="輪郭付きで折り返すキャプション"
<Text x={960} y={900} anchorX={0.5} anchorY={0.5} maxWidth={1400}
  style={{
    fontFamily: 'Hiragino Sans',
    fontSize: 64,
    fontWeight: 700,
    lineHeight: 80,
    align: 'center',
    fill: { type: 'solid', color: '#ffffff' },
    stroke: { paint: { type: 'solid', color: '#57456c' }, width: 8 },
  }}>
  Words that wrap onto a second line when they reach maxWidth.
</Text>
```

- `fontFamily` はインストール済み、または `<Font>` で読み込んだフォントのファミリー名です。指定した `fontWeight` がなければ、CSS と同様に最も近いウェイトを使います。絵文字など不足する文字は別のインストール済みフォントにフォールバックします。JSON プロジェクトでは `font` 素材として同梱することもできます。
- `stroke` はグリフの縁取りです。配列を渡すと、白の内側とキャラクター色の外側のような二重の縁取りを重ねられます：`stroke: [{ paint: outer, width: 14 }, { paint: inner, width: 7 }]`。各 `width` はグリフの輪郭からの太さなので、外側の縁取りほど大きい値になります。配列の順序は問いません。
- `lang` は `fontFamily` が省略された場合や文字が不足する場合のフォールバック言語を選びます。全体には `<Composition lang="ja-JP">` を設定します。子は `lang` を持つ最も近い `Composition`、`Group`、`Sequence` を継承します。字幕、タイムラインのテキスト、`useTextMetrics()`、`useFitText()`、コンポーネントのプレビューにも適用されます。フレームごとに変更でき、計測と描画は同じ言語を使います。`<Text lang="zh-Hant">` は祖先より優先し、明示的な `style.lang` はさらに優先します。空の `lang` はシステムのロケールを選びます。`ja-JP`、`ko-KR`、`zh-Hans`、`zh-Hant`、`zh-Hant-HK` などに対応します。未設定時はシステムのロケールを使い、使用可能な文字はフォントによって決まります。Canvas の言語選択に対応したブラウザでは `lang` を適用します。

```tsx title="日本語を既定にして中国語のグループを配置"
<Composition width={1920} height={1080} fps={30} durationInFrames={90} lang="ja-JP">
  <Text>漢字とかな</Text>
  <Group lang="zh-Hant">
    <Text y={80}>繁體中文</Text>
    <Text y={160} lang="ko-KR">한국어</Text>
  </Group>
</Composition>
```

`prepare()` は `<Composition>` が描画される前に実行されます。計測時の言語は `measureText()` に `lang` として明示してください。例：`measureText('漢字とかな', { lang: 'ja-JP' })`。

未インストールのフォントファイルは `<Font>` で宣言します。相対パスの `src` はエントリーファイルのフォルダから解決され、`fontFamily` はファイル名ではなく内部のファミリー名を使います。TrueType、OpenType、WOFF、WOFF2 に対応し、`src` は `http://` / `https://` の URL でも構いません。

```tsx title="エントリーの隣のフォントを読み込む"
<Composition width={1920} height={1080} fps={30} durationInFrames={90}>
  <Assets>
    <Font src="./fonts/MPLUSRounded1c-Bold.ttf" />
  </Assets>
  <Text style={{ fontFamily: 'M PLUS Rounded 1c', fontWeight: 700, fontSize: 96 }}>
    Hello
  </Text>
</Composition>
```

`src` には Google Fonts の CSS リンクなど、ウェブフォントのスタイルシートも指定できます。`@font-face` のルールごとにフォントファイルを最大 1 つ読み込み、`fontFamily` でスタイルシートのファミリー名を使えます。Celesta が対応する項目を含む最後の `src` を読み、その中で `format()` の指定が TrueType、OpenType、WOFF、WOFF2 のいずれかであるか、指定のない最初の `url()` を使います。EOT、SVG フォント、フォントコレクションと指定された項目や、Celesta が描画できない `tech()`（COLRv1 のカラーグリフなど）の付いた項目は読み飛ばします。そのような `url()` が `src` にないルールは、代わりにインストール済みのフォントで描画します。`local()` の項目を順に試し、フルネーム（`local("Hiragino Sans W6")` など）または PostScript 名（`local(HiraginoSans-W6)` など）が大文字小文字を区別せずに一致する、コンピューターにインストール済みの最初のフォントを使います。ファミリー名では検索しないため、`local("Hiragino Sans")` はどれにも一致しません。`local(Arial)` が一致するのは、Arial のレギュラーのフルネームも「Arial」だからです。どれもインストールされていなければ、そのルールは何も描画せず、文字は別のフォントにフォールバックします。デスクトップアプリと書き出しでは、これをフォントのフォールバック警告として表示します。`local()` の後に `url()` があればそちらを使うため、描画がコンピューターに左右されるのは、スタイルシートにダウンロードするファイルがない場合だけです。そのフォントがインストールされていないコンピューターで書き出すと、見た目が変わります。ブラウザのプレビューでは、ブラウザが独自の規則で名前を照合し、使えるインストール済みフォントを限ることがあります。Safari は OS に付属するフォントにしか一致させません。また、デスクトップアプリと書き出しはルールの `font-weight`、`font-style`、`unicode-range` を無視し、インストール済みのファミリーと同じ名前のスタイルシートのファミリーでも、そのファミリーの他のフェイスを隠しません。ブラウザのプレビューはどちらも反映します。ブラウザと違い、ファイルの読み込みに失敗しても次の項目は試しません。リンクに必要なウェイトをすべて含めてください。不足する場合は提供された最も近いウェイトで描画します。

```tsx title="Google Fonts のファミリーを読み込む"
<Assets>
  <Font src="https://fonts.googleapis.com/css2?family=M+PLUS+Rounded+1c:wght@400;700" />
</Assets>
<Text style={{ fontFamily: 'M PLUS Rounded 1c', fontWeight: 700, fontSize: 96 }}>
  こんにちは
</Text>
```

## 欧文と和文で書体を分ける（和欧混植）

`fontFamily` には書体名の並び（font stack）も指定できます。各文字は、並びの先頭から順に見て、その文字を持つ最初の書体で描かれます。英数字は欧文書体、かなと漢字は和文書体、という組み方ができます。文字と、それに付く結合文字や異体字セレクターは、常に同じ書体で描かれます。

```tsx title="一行の中で欧文書体と和文書体を使い分ける"
<Composition width={1920} height={1080} fps={30} durationInFrames={90} lang="ja-JP">
  <Assets>
    <Font src="https://fonts.googleapis.com/css2?family=Inter:wght@500" />
    <Font src="https://fonts.googleapis.com/css2?family=Noto+Sans+JP:wght@500" />
  </Assets>
  <Text style={{ fontFamily: ['Inter', 'Noto Sans JP'], fontWeight: 500, fontSize: 64 }}>
    Celesta 0.5.0 で字幕の組版を改善
  </Text>
</Composition>
```

インストールも読み込みもされていない書体は飛ばします。基準線は、並びのうち使える最初の書体が決めます。そのため、和文を含む行と含まない行で行間が変わりません。並びのどの書体にもない文字は、`lang` に応じて選ばれる OS の書体にフォールバックします。既定では `<Font>` で読み込んだ書体はフォールバック先に選ばれないので、並びに加えるか、後述の `fontFallback` を指定してください。計測と折り返しも並びに従います。`<Code>` にも並びを指定できます（`style={{ fontFamily: ['JetBrains Mono', 'Noto Sans JP'] }}`）。`<Span style={{ fontFamily }}>` も並びを受け付けます。書き出し時は、並びのどの書体もないときと、並びのどの書体にもない文字があるときに警告します。ブラウザーのプレビューは並びを CSS の font-family としてブラウザーに渡します。そのため、並びのどの書体にもない文字は、書き出しと見た目が変わることがあります。

- `maxWidth` は単語の境界で折り返し、`align` はその幅の中に各行を配置します。`maxWidth` がない場合、`align` は最も長い行の幅の中に各行を配置します。文字列に `\n` を入れると任意の位置で改行できます。
- 日本語は通常、多くの文字の間で折り返すため「フレーム」のような単語も分かれます。禁則処理により「、」「。」などは行頭に来ません。スタイルに `lineBreak: 'phrase'` を指定すると、[BudouX](https://github.com/google/budoux) の日本語モデルで検出した句の境界だけで折り返します。`lang` は現在フォールバックフォントの選択だけに使われます。句が `maxWidth` より長い場合は通常どおり内部で折り返します。ブラウザのプレビューは `lineBreak` を無視し、空白でのみ折り返します。
- 単一行のテキストは見える字形を縦方向の基準にするので、`anchorY={0.5}` は見えない行ボックスではなく文字自体を中央に置きます。幅は送り幅なので、前後の空白も幅を持ちます。改行や maxWidth での折り返しで複数行になったテキストは、measureText と同じ行ボックスを基準にします。
- `anchorY="baseline"` は最初の行のベースラインをアンカーにします。同じ `y` を持つテキストは、文字やサイズによらずベースラインが揃います。

### 読み込んだ書体にフォールバックする

`fontFallback="loaded"` を指定すると、`<Font>` で読み込んだ書体を OS の書体より先に探します。並びに毎回書く必要はありません。`fontFamily` のどの書体にもない文字は、`<Font>` を書いた順に見て、その文字を持つ最初の読み込み書体で描かれます。どれにもない文字だけが OS の書体で描かれます。そのため、書き出す環境にインストールされた書体によって字形が変わりません。

```tsx title="和文を OS の書体ではなく読み込んだ書体で描く"
<Composition width={1920} height={1080} fps={30} durationInFrames={90} lang="ja-JP" fontFallback="loaded">
  <Assets>
    <Font google="JetBrains Mono" />
    <Font google="Noto Sans JP" subsets={['japanese']} />
  </Assets>
  <Text style={{ fontFamily: 'JetBrains Mono', fontSize: 64 }}>let 名前 = "値";</Text>
</Composition>
```

- `<Group>` と `<Sequence>` にも `fontFallback` を指定できます。子は `lang` と同じく最も近い指定を受け継ぎ、`style.fontFallback` がさらに優先します。既定は `'system'` です。
- `fontFamily`（テキスト自身か `<Span>` のもの）を指定したテキストにだけ効きます。指定のないテキストは OS の既定の書体のままです。
- `prepare()` 内の `measureText()` と `fitText()` はコンポジションの外で動くため、`lang` と同じく、スタイルに `fontFallback: 'loaded'` を明示してください。
- 計測、折り返し、基準線も描画と同じ選び方に従います。
- 読み込んだ書体で描いた文字は、書き出し時に警告しません。どの読み込み書体にもない文字は、今までどおり警告します。
- ブラウザーのプレビューは、読み込んだ書体を CSS の font-family の末尾に加えます。ブラウザーはクラスタ単位ではなく文字単位で書体を選ぶため、結合文字が元の文字と別の書体で描かれることがあります。

## 文章の一部を強調する

`<Span>` は文章の一部の `fill`、`fontWeight`、`fontFamily`、`fontFeatures` を変えます。文章全体は一つの段落として組まれたままで、折り返し・揃え・計測も一つの段落として行われ、基準線は文章自身の文字が決める位置から動きません。Span は入れ子にでき、書体にないウェイトは最も近いウェイトで描かれます。

```tsx title="一つの段落の中で一語を強調する"
<Text style={{ fontSize: 48, fill: { type: 'solid', color: '#ffffff' } }}>
  速い、<Span style={{ fill: '#28A34A', fontWeight: 700 }}>カンタン</Span>、頼もしい。
</Text>
```

Span は `<Text>`、`<TextBox>`、`<TextReveal>`、`<Dialogue>`、`DialogueSeries` の台詞と、`useTextMetrics()`、`measureText()`、`useFitText()`、`fitText()`、`useTypewriter()` で使えます。打鍵の長さを `visibleCharacters` に渡すと、組版を変えずに装飾付きの文章を打鍵表示できます。Span を返すコンポーネントは文章の中では使えません。日本語・中国語・韓国語の文章で Span を使うときは `lang` を指定してください。Span は shaping を分割するため、言語の指定がないと、Span の隣の文字が別の書体で描かれる場合があります。ブラウザーのプレビューは Span ごとにその書体で描くため、Span の境目で書き出し結果とわずかに異なる場合があります。

## 合字などの OpenType 機能を切り替える

`fontFeatures` は、CSS の `font-feature-settings` と同じように、書体の OpenType 機能を切り替えます。JetBrains Mono のようなコード用の書体は、`liga` と `calt` の機能で `===` や `->`、`!=` を一つの記号にまとめます。コードを書いたとおりに見せるときは、これらを切ります。

```tsx title="合字を使わず、コードを書いたとおりに描く"
<Assets>
  <Font google="JetBrains Mono" weights={[400]} />
</Assets>
<Text style={{ fontFamily: 'JetBrains Mono', fontSize: 48, fontFeatures: { liga: false, calt: false } }}>
  {'a === b'}
</Text>
```

キーは `liga`、`calt`、`kern`、`zero`、`tnum`、`ss01` などの 4 文字の機能タグで、値は `true`、`false`、または代替字形の番号（`salt: 2`）です。書体にない機能を指定しても何も変わりません。タグや値の形が正しくないとエラーになります。`<Span style={{ fontFeatures }}>` は文章の一部の機能を、単語の途中でも切り替えます。指定はタグごとに文章の指定に重なります。計測も同じ機能の指定で字形を決めるため、計測した幅と描いた幅は一致します。`<Code>` は書体の合字のまま描きます。コードを書いたとおりに見せるときは、`style` に同じ `fontFeatures` を指定します。

ブラウザーのプレビューは、`<Font>` で読み込んだ書体（ファイルから読み込んだものと、スタイルシートが `local()` で描画するもの）にだけ機能を適用し、`fontFeatures` を指定した文章がインストール済みの書体をファミリー名で使う場合や `fontFamily` を指定していない場合はエラーになります。書き出しではインストール済みの書体にも適用されます。

## 文字を測る・枠に収める

デスクトップアプリと CLI では、prepare() 内で非同期の measureText(text, style, options)、描画中に useTextMetrics(text, style, options) を使えます。どちらも Text と同じ組版で、幅、高さ、行数、ベースライン、文字ごとの位置を返します。折り返す幅は maxWidth で指定します。prepare() の時点では Font がまだ読み込まれていないため、options.fonts にフォントファイルを指定してください。フックは現在の言語設定と Font の宣言を引き継ぎます。

デスクトップアプリと CLI で利用できます。TextBox は、width、height、必要に応じて maxLines に収まるよう、minFontSize から maxFontSize の範囲で文字サイズを選びます。x/y は枠の左上、verticalAlign は top、middle、bottom です。最小サイズでも収まらない場合の overflow は clip（既定）、visible、error から選べます。描画せずに調整後のスタイルと fits を得るには、prepare() で fitText()、描画中に useFitText() を使います。

```tsx file=examples/fitted-text.tsx title="fitted-text.tsx"
```
