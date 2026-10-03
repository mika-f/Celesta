# 36 DAYS

このリポジトリ自身の Git 履歴から作った、44 秒のデータ映像です。
最初のコミットから 36 日間のコミット数、コードの量、マイルストーンを、120 BPM のオリジナルスコアに合わせて見せます。

`@celesta/react` のモーション用ヘルパー（`Series`・`Stagger`・`useBeat`・`useCue`・`TextReveal`・
`useTypewriter`・`useCountUp`・`Camera`・`Polyline` など）は、この映像を一度素の API だけで作り、
何度も手書きしたものを切り出して作られました。各シーンがそのまま使用例になっています。

- `film.tsx` — Celesta の File → Open… で開く React ソース（エントリ）。シーンは `scenes/`、共通部品は `components/`、シーンの並びは `timeline.ts`、Git 履歴から取った数値は `data.ts` にあります。
- `make-score.py` — BGM を生成する Python スクリプト。標準ライブラリのみ。
- `poster.jpg` — 書き出した映像から抽出した静止画。

![poster](poster.jpg)

## 構成

120 BPM（1 拍 = 15 フレーム、1 小節 = 60 フレーム）。シーンはすべて小節の頭で切り替わり、`<Series>` で長さ順に並べています。

| 時間 | 内容 | 主なヘルパー |
| --- | --- | --- |
| 0–4 秒 | `git log` を打ち込み、流れるログの 1 行目へカメラが飛び込む | `useTypewriter`・`Camera` |
| 4–8 秒 | タイトル「36 DAYS OF CELESTA」。背景のドットが拍に合わせて光る | `TextReveal`・`useBeat`・`random` |
| 8–14 秒 | コミット数・クレート数・行数が 1 拍ずつカウントアップ | `Stagger`・`useCountUp` |
| 14–22 秒 | 日ごとのコミット数の棒グラフと、累計の折れ線 | `Polyline`・`pointOnPolyline`・`useCue` |
| 22–32 秒 | タイムラインをカメラが移動し、6 つのマイルストーンを順に見せる | `useCue`・`Camera`（`shake`）・`TextReveal` |
| 32–38 秒 | クレートごとのソース行数のランキング | `Stagger`・`useCountUp` |
| 38–44 秒 | ロゴと「next: higher-level components & hooks」 | `TextReveal`・`useTypewriter`・`noise` |

HUD のシーン名とタイムコードは `computeSeries()`・`cueAt()`・`frameToTimecode()` で、各シーンの退場は
`<Transition type={['fade', 'slide']} direction="out">` で描いています。

## データ

数字は 2026-09-29 時点のこのリポジトリから取ったもので、`data.ts` に取得コマンドと一緒に書いてあります。

- 日ごとのコミット数: `git log --no-merges --date=short --format=%ad | sort | uniq -c`
- 総コミット数: `git rev-list --count HEAD`
- 行数: `crates/*` の `.rs` と `packages/react/src` の `.ts`（生成コードを除く）を `wc -l`

## 再生成

WAV と MP4 はリポジトリ全体の設定で Git の管理対象外です。クローン後は最初にスコアを生成してください。

```sh
python3 examples/36-days/make-score.py
node skills/celesta/scripts/inspect.mjs examples/36-days/film.tsx --every 30
Celesta-export --react examples/36-days/film.tsx 36-days.mp4
```

ソースから実行する場合は `Celesta-export` を `cargo run -p celesta-exporter --release --` に置き換えます。
フォント（Archivo Black、JetBrains Mono、Noto Sans JP。いずれも SIL Open Font License）は初回に Google Fonts から取得され、以降はキャッシュされます。
映像とスコアはこのリポジトリのためのオリジナルの手続き的制作です。
