コードを一文字ずつ表示すると入力の雰囲気が出ます。この章は [`Code`](/ja/docs/code/) を入力風に表示し、カーソルを置き、その基礎の低レベルツールを説明します。

## 入力風のコード表示

`useTypewriter()` の `length` を `visibleCharacters` に渡します。

```tsx title="ソースを入力風に表示"
import { useTypewriter } from '@celesta.studio/text';
import { Code } from '@celesta.studio/code';

function Typed() {
  const { length } = useTypewriter(source, { from: 15, framesPerChar: 0.5 });
  return <Code x={64} y={64} language="ts" style={style} visibleCharacters={length}>{source}</Code>;
}
```

ソースか言語が変わったときに全体を一度だけトークン化し、入力効果はその結果を段階的に見せます。色は**完成した**コードに対応するので、途中の文字列も文字列色になり、引用符を閉じてもちらつきません。

## visibleCharacters が数えるもの

- `useTypewriter().length` と同じ、Unicode の**元ソースのコードポイント**です。単一の絵文字は 1、結合・旗・肌色などの並びはコードポイントごとに数えます。
- タブと LF はそれぞれ 1 文字です。**CRLF は 2 文字**です。
- 小数は切り捨て、負数では何も表示せず、既定の `Infinity` ではすべて表示します。

## 入力を追うカーソル

`useCodePoint(source, { line, column }, style?, tabSize?)` は `Code` と同じフォントとタブ展開で位置を計測し、カーソル・下線・注釈を置けます。行と列は **1 始まり**で、列は元の文字数です（タブや単一の絵文字も 1 列）。行末に置くため最後の文字の次も指定できます。無効な位置はエラーになります。

```tsx title="入力末尾で点滅するカーソル"
import { Group, Rect } from '@celesta.studio/react'; import { useTypewriter } from '@celesta.studio/text';
import { Code, useCodePoint } from '@celesta.studio/code';

function TypedWithCaret() {
  const { length, caretVisible } = useTypewriter(source, { from: 15, framesPerChar: 0.5 });
  const typed = Array.from(source).slice(0, length).join('').split(/\r\n|\r|\n/);
  const caret = useCodePoint(source, {
    line: typed.length,
    column: Array.from(typed[typed.length - 1]).length + 1,
  }, style);

  return (
    <Group x={64} y={64}>
      <Code language="ts" style={style} visibleCharacters={length}>{source}</Code>
      {caretVisible && <Rect x={caret.x} y={caret.y} width={2} height={caret.lineHeight} fill="#a68bbf" />}
    </Group>
  );
}
```

両方を同じ `Group` に入れると、移動・拡縮しても位置が揃います。フックは `Code` の左上を基準に、変換前の `x`、行の上端 `y`、`baseline`、`lineHeight` を返します。

## 行単位で表示

`codeCharacterCount(source, { line, column })` はソース位置をその手前の文字数に変換し、`visibleCharacters` に渡せます。通常の関数なので React の外でも使えます。4 行のコードの最初の 3 行を表示するには、以下を使います。

```tsx title="最初の 3 行を表示"
import { Code, codeCharacterCount } from '@celesta.studio/code';

function FirstThreeLines({ source }: { source: string }) {
  const count = codeCharacterCount(source, { line: 4, column: 1 });
  return <Code language="ts" visibleCharacters={count}>{source}</Code>;
}
```

行末の位置は改行を含みません。含めるには次の行の列 1 を使います。[`useCue`](/ja/docs/motion-toolkit/) と組み合わせると、1 行に 1 キューで段階的にコードを表示できます。

## トークン

`tokenizeCode(source, language?)` は `Code` のトークンを返します。各トークンは元の `text`、`type`（twinkleplop の名前または `plain`）、UTF-16 単位の `start` と `end`（終了位置は含まない）を持ちます。空白も含め全ソースを保持し、HTML や DOM は不要です。独自の描画やコード分析に使えます。

```tsx title="コメントを数える"
import { tokenizeCode } from '@celesta.studio/code';

const comments = tokenizeCode(source, 'ts').filter((token) => token.type === 'comment');
```

## 行を自分で描く

`highlightCode(source, { language, theme, tabSize })` は `Code` が描く行と色の区間を返します。色分けはそのままに、1 行ずつ拍に合わせて打ち込む、行の横に印を付ける、帯を画面の端まで伸ばす、といった見せ方を自分で組めます。規則と既定値は `Code` と同じです（`'text'`、`codeThemes.dark`、タブ幅 `2`）。LF・CRLF・CR で行が分かれ、タブはタブ位置まで展開し、テーマにないトークンの種類は `foreground` になります。通常の関数なので、ソースが決まっているならコンポーネントの外で一度だけ呼びます。

**各行**

| API | 用途 |
| --- | --- |
| `text` | タブを展開した行の文字列です。改行は含みません。空行は空文字列で、行の高さは保ちます。 |
| `start` | 行より前にある元のソースの文字数です。`visibleCharacters` と同じ数え方です。 |
| `columns` | 行の元の文字ごとの、展開後の桁位置です。`columns[0]` は `0` で、最後の要素は `text` の長さです。ソースの文字数を、展開後の文字列を数える `Text` の `visibleCharacters` に変換できます。 |
| `runs` | 展開後の文字列に対する `start`・`end`・`color`・`type` を持つ区間です。順に並び、文字列全体を覆います。そのまま `style.colorRuns` に渡せます。隣り合う同じ色のトークンは 1 つの区間にまとまり、最初のトークンの `type` を残します。 |

空でない行を、それぞれ `Code` と同じスタイルの `Text` 1 つで描きます。`Code` は各行のベースラインを、行の上端から `'M'` の ascent だけ下に置きます。この値は `useTextMetrics` で読めます。同じように置けば `Code` と同じ絵になります。次の例は 1 行ずつ拍に合わせて打ち込みます。

```tsx file=examples/code-lines.tsx title="1 行ずつ打ち込む"
```

代わりに `<Span style={{ fill: run.color }}>` で色を付けるときは、各区間の開始と終了で `Array.from(line.text)` を切り出します。1 つの Text には `colorRuns` と Span の fill のどちらか一方しか使えません。

`highlightCode` は文字の幅を知りません。カーソルや行の文字の終わりなどの位置は計測します。ソース上の位置なら `useCodePoint()`、行の中の位置なら `useTextMetrics()` と `textCaret()` を使います。

`codeLines(tokens, tabSize?)` は色を付ける前の段階です。`tokenizeCode()` のトークンを同じ行に分け、隣り合う同じ種類のトークンごとに色のない区間を返します。区間は `type`、タブを展開した `text`、展開後の開始位置 `column` を持ちます。色でまとめる前のトークンの種類が必要なときに使います。

## 制限

- 空でない行はそれぞれ色の区間を持つ `Text` レイヤー 1 つです。打ち込み中は、1 文字増えるたびにその行を描き直します（字形処理はキャッシュされます）。画面に収まる量にしてください。
- カーニング・合字・結合文字は色の境界をまたいで字形処理されます。字形のまとまりは最初の文字の色になり、まとめて表示されます。GPU のテクスチャ上限より幅の広い行は縮小して描かれ、ぼやけることがあります。
- プロポーショナルフォントでの配置は保証されません。
- 空行は `Text` レイヤーを作りませんが、行の高さは保ちます。

ブラウザ対応は [Issue 100](https://github.com/mika-f/Celesta/issues/100) で追跡しています。
