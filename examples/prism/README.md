# PRISM — Make your ideas move.

Celesta の機能を、映像そのものの動きで紹介する48秒のショートフィルム。
アイボリー、黒、朱色を基調に、三角形のプリズムと大きなタイポグラフィを
120 BPM のオリジナル電子音楽に合わせて展開します。

- `prism.mp4` — 完成映像。1920 × 1080 / 30 fps / H.264 + AAC / ステレオ。
- `film.tsx` — Celesta の File → Open… で開く編集可能な React ソース（エントリ。チャプターは `scenes/`、部品は `components/`、立ち絵と音声の読み込みは `character.ts`）。
- `poster.jpg` — 実際に書き出した映像から抽出した静止画。
- `make-score.py` — 音楽を再生成する Python スクリプト。追加ライブラリ不要。
- `assets/fonts/` — Bebas Neue と IBM Plex Mono。OFL ライセンスを同梱。

## 構成

| 時間 | 内容 |
| --- | --- |
| 0–2秒 | スリットが開き、プリズムが現れる |
| 2–6秒 | CELESTA / MAKE YOUR IDEAS MOVE. |
| 6–11秒 | React のコードと、変更に応じて色が変わるプレビュー |
| 11–16秒 | `spring` と `interpolate` で動く図形 |
| 16–21秒 | 生成グラフィック、文字、音楽のレイヤー |
| 21–26秒 | カスタムフォント・縁取り文字・Gridレイアウト |
| 26–32秒 | PSD立ち絵・字幕・音声からの自動リップシンク |
| 32–37秒 | 前後へのスクラブと、フレーム番号に対応する映像 |
| 37–40秒 | WRITE. / PLAY. / REPEAT. のビートカット |
| 40–44秒 | MP4 書き出しの紹介 |
| 44–48秒 | ロゴとコピー、余韻を残す終止 |

コード画面・プレビュー・スクラブ・書き出しの進捗は機能を説明するモーショングラフィックで、
アプリの画面収録や処理速度の測定ではありません。波形も音楽の拍に合わせた
手続き的アニメーションで、音声解析によるスペクトラムではありません。
立ち絵は `Character` / `CharacterView`、字幕と音声は `Dialogue`、口の動きは
`prepare()` 内の `loadLipSync()` で実際に生成しています。字幕は日本語音声の英訳です。
BGMはセリフの前後で音量を下げます。フォント・縁取り・Grid・Transition も
実際の Celesta コンポーネントを使っています。JSON Composition の紹介は含みません。

## 再生成

リポジトリのルートから、ビルド済みの Celesta exporter と React runtime を使います。
WAV と MP4 はリポジトリ全体の設定で Git の管理対象外です。
クローン後は最初に音楽を生成してください。

```sh
python3 examples/prism/make-score.py
node skills/celesta/scripts/inspect.mjs examples/prism/film.tsx --every 15
target/release/celesta-exporter --preset medium --crf 17 \
  --react examples/prism/film.tsx examples/prism/prism.mp4
```

既存の出力を置き換える場合は exporter に `--overwrite` を追加します。
インストール版では `target/release/celesta-exporter` を Celesta-export のパスに
置き換えてください。書き出しには利用可能な GPU が必要です。

モーショングラフィックとBGMはオリジナルの手続き的制作です。映像は `film.tsx` のフレーム数、
色、文字、形状を変更して調整できます。全アセットはローカルにあり、再生時の
ネットワーク接続や追加フォントのインストールは不要です。


## 立ち絵と音声

既存のサンプルと同じ素材を相対パスで参照しています。`prism/` を単独で移動するときは
次の素材も一緒に移し、`character.ts` の `PSD` / `PRESET` / `VOICE` を更新してください。

- `../assets/illust/琴葉姉妹_SD立ち絵.psd`
- `../assets/illust/琴葉茜.pfv`
- `../assets/voices/character-lipsync-demo.wav`

立ち絵：[アジサバ「琴葉姉妹SD立ち絵」](https://booth.pm/ja/items/6108134)。
琴葉姉妹は株式会社エーアイのキャラクターです。利用条件は配布元の案内に従ってください。
音声はリポジトリ内の既存リップシンクデモ素材を再利用しています。
MP4/WAVはGit管理対象外なので、別の環境では既存デモの音声も用意してください。
