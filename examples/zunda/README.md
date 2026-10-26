# 動画はコードで書くのだ

ずんだもんと四国めたんが Celesta を紹介する、約 2 分の解説動画です。
真っ黒な `<Composition>` から始まり、二人が話すたびに、この動画自身にレイヤーが 1 枚ずつ増えていきます。

Celesta の File → Open… で `film.tsx` を開くとプレビューできます。`src/` 以下のファイルを保存しても、プレビューは読み直されます。
音声は `prepare()` が VOICEVOX Engine で作ってキャッシュするので、台本を直すと、変わった行だけが読み直され、
字幕・口パク・シーンの切り替えが新しい音声の長さに追従します。

## ファイル構成

```
film.tsx              エントリ。prepare() と、全体の重なり順の組み立てだけ
script.ts             台本：話者・表情・VOICEVOX のスタイル・読み・シーン
src/
  theme.ts            画面サイズ・fps・色・書体・素材の場所
  timing.ts           台本を VOICEVOX で読み上げ（prepareNarration）、音声の長さから全台詞のフレームを決める（planDialogue）
  camera.ts           動画全体のカメラワーク
  fonts.ts            和文フォントを使う文字だけに絞って読み込む URL
  cast/               立ち絵と台詞
    portraits.ts        PSD のどのレイヤーをどの表情・目・口で見せるか
    acting.ts           いつどの表情か、跳ね、ゆらぎ
    speakers.ts         話者の名前と色
    Cast.tsx            キャラクターの宣言、立ち絵の表示、台詞の再生（DialogueSeries）
  scenes/             シーンごとに 1 ファイル（index.tsx が並べて描く）
    void.tsx layers.tsx effects.tsx path.tsx timeline.tsx voice.tsx rewind.tsx agent.tsx outro.tsx
    backdrop.tsx        冒頭からエフェクトのシーンまで育っていく背景
  ui/                 シーンをまたいで使う部品（コードパネル、字幕の帯、チャプター表示、ワイプ など）
  lib/shapes.ts       Path に渡す点列の図形
prepare-assets.ts     立ち絵と映像素材を取得して assets/ に置く（素材はリポジトリに含めない）
make-score.py         BGM を生成する（Python の標準ライブラリのみ）
```

## このサンプルの書き方

ほかの動画を作るときにも使える約束ごとです。

- **タイミングは 1 か所で決める。** 台詞とシーンのフレームは `src/timing.ts` が `planDialogue()` で決め、ほかは
  `line('s2').from` や `partway('e2', 0.3)`（台詞の途中）、`sceneStart('voice')` で参照します。数字のフレームを直書きしないので、
  台本や音声を変えても崩れません。計画は `prepare()` のあとにしかできないので、台詞から決まるフレームは
  モジュールの読み込み時ではなく、描画中に関数で計算します（`backdropCues()` など）。
- **シーンの中でも動画全体のフレームで考える。** シーンは自分の `<Sequence>` の中で描かれますが、`useFilmFrame(scene)` で
  全体のフレームを取り、台詞のフレームとそのまま比べます。
- **シーンは `SceneDefinition` を 1 つ export する。** `Stage` はカメラの内側（立ち絵と一緒に寄る）、`Overlay` は画面に固定、
  `prepare` は読み込み、`strings` はそのシーンが描く和文です。`scenes/index.tsx` に登録すると、チャプター表示（エディタの
  チャプターの目印も）・ワイプ・`prepare()`・和文フォントの絞り込みが自動でつながります。
- **台詞は `<DialogueSeries>` に並べさせる。** 字幕の帯と名前札は `subtitle.render` で描き、`holdSubtitle` で台詞の合間も
  帯を出し続けます。改行は `lineBreak: 'phrase'`（文節の途中で折り返さない）に任せ、文の切れ目で改行したい行だけ、
  台本の `text` に `\n` を書きます。
- **立ち絵の表情は PSD の `expressions` で表す。** 表情はビュー（`<Cast>`）側で 1 か所で決め、台詞は口パクだけを担当します。
  まばたきは表情ごとの `blink` に目のレイヤー（開・閉）を渡して Celesta に任せます。
  四国めたんのようにポーズごとに PSD が分かれている場合は、ポーズごとに `<Character>` を作ってビューの `character` を差し替えます。
- **過去のフレームを描き直すには `<FrameRecall frame>`（中身は `<FreezeFrame>`）。** 描く中身は `WorldContext` で渡すので、
  シーンが自分を含む世界を import して循環することがありません。
- **型チェック。** リポジトリのルートで `pnpm run build:runtime` のあと `pnpm run typecheck:examples zunda`。

## 構成

| 時間 | シーン | 内容 | 主な機能 |
| --- | --- | --- | --- |
| 0:00 | 真っ暗 | 空のコンポジションのコードが打ち込まれ、左上のコードパネルになる | `Sequence`、`interpolate`、`@celesta/code` の `Code` |
| 0:08 | レイヤーを重ねる | 台詞に合わせて背景が `Rect` → グラデーション → 写真と育つ | `Rect`、線形グラデーション、`Image fit="cover"`、`Motion` |
| 0:30 | エフェクト | タイトルが光り、影が付き、背景がぼける。コードの数値も同時に動く | `glow`・`shadow`・`blur`、`spring` |
| 0:41 | Path で描く | ずんだ餅とえだまめが一筆書きで描かれる | `Path`、`Polyline progress`、`pointOnPolyline` |
| 0:51 | 動画とタイムライン | クリップがトラックに落ち、実写の映像が流れる | `Video`、下帯コンポーネント |
| 1:01 | 声と口パク | ずんだもんの口元に寄り、いまの母音と波形、PSD レイヤー名を表示。表情も次々に切り替わる | `prepareNarration`・`lipSyncFromVoicevox`、`decodeWav`・`buildEnvelope`、PSD 立ち絵の `expressions`、`Camera` |
| 1:19 | フレームは関数 | 過去のシーン 4 つを、立ち絵ごとサムネイルとしてその場で描き直し、前後にスクラブする | `FreezeFrame`（`src/ui/FrameRecall.tsx`） |
| 1:30 | AI エージェント | この動画を作ったエージェントの作業ログ | |
| 1:52 | おわり | ロゴ、コピー、クレジット | `TextReveal` |

BGM は台詞の間だけ自動で下がります（`<Audio duck={{ under: 'voice' }}>`）。

## 本物と演出

- **実際の機能**：台本は `prepare()` の `prepareNarration()` が VOICEVOX Engine で読み上げ、口の形はその AudioQuery の
  モーラの長さから作っています（声のシーンの読みと母音も同じ AudioQuery から）。台詞は `<DialogueSeries>` が音声の長さどおりに並べます。
  立ち絵は公式 PSD（縮小しただけのもの）を `Character` の `portrait: { type: 'psd' }` で使い、表情は PSD の表情フォルダを
  `expressions` に割り当てたもの、まばたきは表情ごとの `blink` による目のレイヤーの切り替えです。
  声のシーンの波形は WAV から読んだ実データです。「フレームは関数」のサムネイルは、録画ではなく過去のフレームの
  背景・シーン・立ち絵を `<FreezeFrame>` でその場で描き直しています。
  タイムラインのシーンのモニターに流れるのは実際の `<Video>` です。
- **演出**：コードパネルの内容は各シーンで実際に使っている API を要約したもので、ソースの行そのままではありません。
  タイムラインのトラック表示とエージェントのログは機能を説明するための描画です。ログの数字（台本の行数、フレーム数）は
  台本とタイミングから計算しています。

## 再生成

WAV・MP4 と `assets/` はリポジトリに含めません。クローン後、リポジトリのルートで依存を入れ（`pnpm install`）、
次の順に実行します。`prepare-assets.ts` は `.ts` のまま実行するので、Node.js 22.18 以降（23 系なら 23.6 以降）が必要です。

```sh
node examples/zunda/prepare-assets.ts        # 立ち絵（PSD を縮小して保存）と映像素材。ffmpeg コマンドが必要
python3 examples/zunda/make-score.py         # BGM
docker run --rm -p 127.0.0.1:50021:50021 voicevox/voicevox_engine:cpu-ubuntu24.04-latest
Celesta-export --react examples/zunda/film.tsx zunda.mp4 --loudness streaming
```

音声は書き出し（またはプレビュー）の `prepare()` が作り、`examples/zunda/.celesta/voices` にキャッシュします。
VOICEVOX Engine が要るのは、初回と、台詞の読みやスタイルを変えたときだけです。Engine の場所は `VOICEVOX_URL` で変えられます。
`--loudness streaming` は、書き出しの音量を動画配信で一般的な -14 LUFS にそろえます。
ソースから実行する場合は `Celesta-export` を `cargo run -p celesta-exporter --release --` に置き換えます。

公式 PSD は 4832 × 9488 ピクセルあります。この動画では縮めて映すので、読み込みと合成を軽くするため、
`prepare-assets.ts` がレイヤーごとに 1/4（めたんは 1/5）に縮小した PSD を書き出します。

## 素材とクレジット

- 音声：VOICEVOX:ずんだもん、VOICEVOX:四国めたん
- 立ち絵：[東北ずん子・ずんだもんプロジェクト 公式イラスト](https://zunko.jp/con_illust.html)（ずんだもん `zunmon008.psd`、四国めたん `met_s214`・`met_s215`・`met_s219`・`met_s220`）。
  [キャラクター利用ガイドライン](https://zunko.jp/guideline.html)に従い、非商用で使用しています。
- 映像素材：[Mixkit](https://mixkit.co/)（[Mixkit Stock Video Free License](https://mixkit.co/license/#videoFree)）。写真として使う静止画も同じ映像から切り出しています。
- フォント：M PLUS Rounded 1c、Dela Gothic One、JetBrains Mono（いずれも SIL Open Font License、Google Fonts から配信）
- BGM・映像：このリポジトリのためのオリジナルの手続き的制作です。
