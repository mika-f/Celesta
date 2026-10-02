# FEATURE TOUR

Celesta の機能を、9 つのチャプターで紹介する 52 秒のモーショングラフィック。
インク黒とウルトラマリン、ワイドな Unbounded と明朝的な Instrument Serif、
和文の Noto Sans JP を 120 BPM のオリジナルスコアに合わせて展開します。
各チャプターは見出し・日本語の説明・それを実現する API・その場で動くデモの 4 つで構成されています。

- `film.tsx` — Celesta の File → Open… で開く React ソース（エントリ）。冒頭とインデックスとアウトロは `scenes/`、9 つのチャプターは `chapters/`（`chapters/index.ts` に一覧、デモは 1 ファイル 1 チャプター）、共通部品は `components/` にあります。
- `make-score.py` — BGM を生成する Python スクリプト。標準ライブラリのみ。
- `poster.jpg` — 書き出した映像から抽出した静止画。

![poster](poster.jpg)

## 構成

120 BPM（1 拍 = 15 フレーム、1 小節 = 60 フレーム）。チャプターはすべて小節の頭で切り替わります。

| 時間 | 内容 |
| --- | --- |
| 0–2 秒 | フレームの中で青いボールが拍ごとに弾み、最後は画面いっぱいに広がってタイトルへ |
| 2–4 秒 | タイトル「Celesta」 |
| 4–8 秒 | INDEX — 9 機能の目次。01 の行がそのまま次の画面へ開く |
| 8–12 秒 | 01 REACT — Rect・Group・Text のレイヤーがアイソメトリックに分解される |
| 12–16 秒 | 02 MOTION — easeOutExpo / easeInOutBack / spring() の曲線と、それで動く図形 |
| 16–20 秒 | 03 LAYOUT — 12 枚のタイルが Grid → Stack → Center と組み替わる |
| 20–24 秒 | 04 TYPE — 1 拍ごとに 8 書体・縁取りを切り替えるタイプ見本 |
| 24–28 秒 | 05 TIMELINE — `.celesta.json` のトラックと、落ちてくるクリップ |
| 28–34 秒 | 06 VOICE — PSD 立ち絵、字幕、音声からの自動リップシンク、実波形 |
| 34–38 秒 | 07 PREVIEW — 前後にスクラブすると、フレーム番号どおりの絵が返るプレビュー |
| 38–42 秒 | 08 EXPORT — フレームのモザイクが埋まっていく MP4 書き出し |
| 42–46 秒 | 09 AGENTS — Skill を読んだエージェントがこの動画を作った記録 |
| 46–52 秒 | ロゴ、タグライン、チャプター番号が 1 つずつ点灯して終止 |

チャプターの境目は 12 本のスラットが中央から走るシェブロン型のワイプで切り替わります。

## 本物と演出

- **実際の機能**: `Composition`・`Sequence` による構成、`interpolate`・`spring`・`Easings`、
  Google Fonts の `<Font>` 読み込み（和文は `text=` で使う文字だけにサブセット）、
  文字の縁取り、`Character` / `CharacterView` / `Dialogue` による PSD 立ち絵と字幕、
  `prepare()` 内の `loadPsdPreset()` / `loadLipSync()`、`decodeWav()` / `buildEnvelope()` で
  音声ファイルから読んだ波形、`<Audio volume>` のキーフレームによるセリフ中の BGM ダッキング。
- **モーショングラフィックとしての演出**: プレビュー画面、タイムライン、書き出しの進捗、
  エージェントのログは機能を説明するための描画で、アプリの画面収録や実際の処理時間ではありません。
  LAYOUT のタイル移動は Grid / Stack / Center の配置を再現した手計算の補間です。
  ログの文言は実際の作業（Skill を読み、`inspect.mjs` で検証し、書き出す）に沿っています。

## 再生成

WAV と MP4 はリポジトリ全体の設定で Git の管理対象外です。クローン後は最初にスコアを生成してください。

```sh
python3 examples/feature-tour/make-score.py
node skills/celesta/scripts/inspect.mjs examples/feature-tour/film.tsx --every 15
Celesta-export --react examples/feature-tour/film.tsx examples/feature-tour/feature-tour.mp4
```

ソースから実行する場合は `Celesta-export` を
`cargo run -p celesta-exporter --release --` に置き換えます。
フォントは初回に Google Fonts から取得され、以降はキャッシュされます。

## 素材

立ち絵と音声は既存のサンプル素材を相対パスで参照しています。
`feature-tour/` を単独で移動するときは次の素材も一緒に移し、`voice.ts` の `PSD` / `PRESET` / `VOICE` を更新してください。

- `../assets/illust/琴葉姉妹_SD立ち絵.psd`
- `../assets/illust/琴葉茜.pfv`
- `../assets/voices/character-lipsync-demo.wav`

立ち絵：[アジサバ「琴葉姉妹SD立ち絵」](https://booth.pm/ja/items/6108134)。
琴葉姉妹は株式会社エーアイのキャラクターです。利用条件は配布元の案内に従ってください。

フォント：Unbounded、Instrument Serif、JetBrains Mono、Noto Sans JP、Dela Gothic One、DotGothic16
（いずれも SIL Open Font License、Google Fonts から配信）。
映像とスコアはこのリポジトリのためのオリジナルの手続き的制作です。
