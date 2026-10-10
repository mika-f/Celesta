# CELESTA TIPS（縦型ショート）

[English](README.md) | 日本語

Celesta の小技を 1 本に 1 つずつ紹介する、縦型ショート動画のテンプレート。
上半分にハイライトしたコード、下半分にそのコードが描く結果をその場で動かして見せます。
1080 × 1920 / 30 fps / 24 秒、120 BPM のオリジナルスコアに合わせて展開します。

ひとつのソースから何本も書き出せます。`properties.ts` が入力（Tip の種類、タイトル、コード、アクセントカラーなど）を
`defineProjectProperties()` で宣言し、`variants/` の JSON が Tip ごとの値を持ちます。

- `film.tsx` — Celesta の File → Open… で開く React ソース（エントリ）。
- `properties.ts` — テンプレートの入力。`plan.ts` — コードの配置と各行の出現時刻、`prepare()` での検証。
- `demos/` — Tip ごとのデモ（1 ファイル 1 デモ、`demos/index.ts` に ID ごとの一覧）。
- `scenes/` — Hook・Build・Outro の 3 シーン。`components/` — ヘッダー、コード欄、デモの時計、背景。
- `variants/` — Tip ごとの値。`make-score.py` — BGM を生成する Python スクリプト（標準ライブラリのみ）。
- `poster.jpg` — 書き出した映像から抽出した静止画。

![poster](poster.jpg)

## 構成

120 BPM（1 拍 = 15 フレーム、1 小節 = 60 フレーム）。シーンは `<TransitionSeries>` でつなぎ、
切り替えは小節の頭に重なりの中心が来るように置いています。尺はどの variant でも同じです。

| 時間 | 内容 |
| --- | --- |
| 0–2 秒 | 完成形。コード全文と、すでに動いているデモの上に、フックのひとこと（`hook`）の帯 |
| 2 秒 | 左からのワイプで、空のエディタへ |
| 2–16 秒 | コードが拍に合わせて 1 行ずつ打ち込まれ、行がそろうたびに下のデモが組み上がる |
| 16–24 秒 | クロスフェードで、コード欄が締めのひとこと（`closing`）に替わる。デモは動き続ける |

## variants

| ファイル | Tip | デモ |
| --- | --- | --- |
| `variants/spring.json` | interpolate と spring の違い | 同じ 0 → 1 の移動を 2 つの関数で。値の読み出しと、1 小節分のカーブ |
| `variants/transition.json` | `<TransitionSeries>` によるシーンのつなぎ | 本物の `TransitionSeries`（Sun → wipe → Moon）を毎小節再生し、重なりをタイムラインで示す |
| `variants/phrase.json` | 日本語の文節改行（`lineBreak: 'phrase'`） | 同じ文・同じ `maxWidth` で、`'normal'` と `'phrase'` の折り返しを並べる |

`variants/en/` には同じ 3 本の英語版があります。違うのは `title`・`hook`・`closing` だけで、コードとデモは共通です
（phrase のデモは日本語の改行を扱うので、例文は日本語のままです）。
ほかの言語にするときも、この 3 つを書き換えた variant を足せば済みます。
和文用の Noto Sans JP は欧文も含むので、英語の文字もそのまま描けます。
英文は和文より横に長いので、`hook` は 1 行 16 字ほどまでにして、`\n` で改行を指定してください。

各 variant が持つ値は次のとおりです。省いた値は `properties.ts` の既定値（spring の Tip）になります。

| キー | 型 | 内容 |
| --- | --- | --- |
| `tip` | select | 下半分に出すデモの ID（`demos/index.ts` のキー） |
| `number` | number | `TIPS #01` の番号 |
| `title` | string | ヘッダーのタイトル。文節で折り返します |
| `hook` | string | 最初の 2 秒に出すひとこと |
| `code` | string | 上半分のコード（TSX としてハイライト）。9 行まで |
| `stages` | string | デモの各段階が始まるコードの行番号（1 始まり、カンマ区切り、昇順）。その行を打ち終えた時点で段階が始まります |
| `closing` | string | 締めのひとこと |
| `accent` | color | アクセントカラー |
| `guides` | boolean | セーフエリア外を赤く塗る確認用の表示。既定は `false` で、完成映像には描きません |

コードの文字サイズは最長の行に合わせて 46〜34 px で決まります。収まらない行や、`stages` の数がデモと合わないときは、
`prepare()` が最初のフレームより前にエラーで止めます。

## Tip を足すには

1. `demos/` にデモを 1 ファイル書きます。描く範囲は下半分（1080 × 960、原点は下半分の左上）です。
   時間は `components/DemoClock.tsx` のフックから取ります。`useStage(i)` は段階 `i` が始まってからのフレーム数を返します。
   `useBarLoop(i)` は、段階 `i` のあとの最初の小節の頭から、小節ごとに繰り返す時計を返します。
   段階の数を `XXX_STAGES` として書き出します。
2. `demos/index.ts` の `DEMOS` に ID・段階数・コンポーネントを登録します。ID は `tip` の選択肢に自動で加わります。
3. `variants/` に JSON を足し、`tip` にその ID を、`stages` にデモの段階数と同じ数の行番号を書きます。

デモに使う値（たとえば spring の `damping`）はコードの断片と同じにしてください。各デモの先頭にある定数がそれです。
冒頭の 2 秒では、すべての段階がとうに始まったものとしてデモが描かれます。デモの側で完成形を別に用意する必要はありません。

## セーフエリア

Shorts・TikTok・Reels は、下の約 20%（y ≥ 1536）にキャプションやチャンネル名を、右端（x ≥ 936）にボタンを重ねます。
背景、コード欄の帯、デモの絵は画面の端から端まで使います。読ませる文字（タイトル、コード、ラベル、ひとこと）は
`constants.ts` の `SAFE` の内側に置いています。確かめるときは `--props '{"guides":true}'` で書き出すと、その外側が赤く塗られます。

## コードの描画について

コードのハイライトは `@celesta/code` の `tokenizeCode()` で行い、1 行を 1 つの `<Text>` として色を `<Span>` で付けています。
`<Code>` コンポーネントは描画中に文字幅を測るため、`inspect.mjs` ではフレームを評価できません。
JetBrains Mono はどの文字も 0.6 em 送るので、ここでは測らずに位置を計算しています。
JetBrains Mono にない和文は `<Span>` で Noto Sans JP を指定して描き、1 em として扱います。
OS のフォールバックに任せないので、どの環境でも同じ字形になります。
そのため `inspect.mjs` で全フレームを確かめられます。

## 再生成

WAV と MP4 はリポジトリ全体の設定で Git の管理対象外です。クローン後は最初にスコアを生成してください。

```sh
python3 examples/shorts-tips/make-score.py
node skills/celesta/scripts/inspect.mjs examples/shorts-tips/film.tsx --every 30 \
  --props-file examples/shorts-tips/variants/spring.json
Celesta-export --react examples/shorts-tips/film.tsx spring.mp4 \
  --props-file examples/shorts-tips/variants/spring.json
Celesta-export --react examples/shorts-tips/film.tsx transition.mp4 \
  --props-file examples/shorts-tips/variants/transition.json
Celesta-export --react examples/shorts-tips/film.tsx phrase.mp4 \
  --props-file examples/shorts-tips/variants/phrase.json
# 英語版
Celesta-export --react examples/shorts-tips/film.tsx spring-en.mp4 \
  --props-file examples/shorts-tips/variants/en/spring.json
```

英語版の残り 2 本も、`variants/en/` のファイルを指定して同じように書き出します。

ソースから実行する場合は `Celesta-export` を
`cargo run -p celesta-exporter --release --` に置き換えます。
同じ値でプレビューするには `celesta-editor examples/shorts-tips/film.tsx --props-file examples/shorts-tips/variants/spring.json` を使います。
フォントは初回に Google Fonts から取得され、以降はキャッシュされます。

## 素材

フォント：Unbounded、JetBrains Mono、Noto Sans JP（いずれも SIL Open Font License、Google Fonts から配信）。
タイトルやひとことは variant ごとに変わるため、和文フォントはサブセットにせず丸ごと読み込みます。
映像とスコアはこのリポジトリのためのオリジナルの手続き的制作です。
