## ブラウザから

[ウェブエディタ](/ja/#playground)で TSX を編集するか開き、**MP4 に書き出す**を選びます。ソース、プレビュー、書き出しは Celesta の React シーン評価器を使います。**メディアを追加**でローカルファイルを名前で追加できます。H.264 の WebCodecs 対応が必要で、音声には AAC エンコードも必要です。プレビューは無音ですが MP4 には一定速度の音声クリップを含められます。文字の計測・自動調整（TextBox、fitText、useFitText、measureText、useTextMetrics）、PSD、プロジェクトのタイムライン、`prepare()`、外部インポート、音声の速度・音量アニメーション、ネイティブ描画にはアプリまたは CLI を使います。

## アプリから

**Export…** で保存先を選びます。拡張子を `.mp4` にすると H.264 の動画、`.mov` または `.webm` にすると透過動画（後述の「透過動画」）になります。それ以外の拡張子は `.mp4` になります。一部だけ書き出すには `I` と `O` で開始・終了を指定します。進捗はステータスバーに表示され、**Cancel export** で中止できます。応答を待っているプラグインがあれば、それも止められます。サウンドトラックをラウドネスの目標値に合わせるには、先に **Export…** の隣の **Loudness** で **Off**、**Streaming (-14 LUFS)**、**Broadcast (-23 LUFS)** のいずれかを選びます。選んだ値はアプリを終了するまで保たれます。書き出しが終わると、完了の通知にサウンドトラックのラウドネスが表示されます（後述の「配信・放送向けのラウドネス」）。書き出し中に出た警告とお知らせ（プレビューしていないフレームのフォントのフォールバックやプラグインのお知らせなど）は、**Problems** タブにひとつの項目として記録されます。それぞれ一度だけ、出た順に、コマンドラインの出力と同じ形で並びます。

## コマンドラインから

アプリにはコマンドライン用の書き出しツールも含まれています。`first-scene.tsx` があるプロジェクトフォルダでターミナルを開き、OS に合ったコマンドを使ってください。

::::tabs[書き出しを行う OS]{default="macOS"}
:::tab[macOS]

以下は Celesta を `/Applications` にインストールした場合の例です。別の場所ならパスを変更してください。

```sh title="React コンポジションを書き出し"
"/Applications/Celesta.app/Contents/MacOS/Celesta-export" --react first-scene.tsx output.mp4
```

:::
:::tab[Windows]

PowerShell を使います。`C:\path\to\Celesta` を Celesta のインストール先または展開先に置き換えてください。インストーラーは Celesta を PATH に追加しません。

```sh title="React コンポジションを書き出し"
& "C:\path\to\Celesta\Celesta-export.exe" --react first-scene.tsx output.mp4
```

:::
::::

出力の拡張子で書き出すファイルの種類が決まります。`.mp4` は H.264 映像と AAC 音声、`.mov` と `.webm` は透過動画（後述の「透過動画」）、`.png`、`.jpg`、`.webp` は静止画（後述の「静止画とコンタクトシート」）です。既存の出力ファイルを置き換えるには `--overwrite` を追加します。`--from` と `--to` で時間範囲を指定できます。

::::tabs[書き出しを行う OS]{default="macOS"}
:::tab[macOS]

```sh title="最初の 1 秒を書き出し"
"/Applications/Celesta.app/Contents/MacOS/Celesta-export" --from 0 --to 1 --react first-scene.tsx section.mp4
```

:::
:::tab[Windows]

```sh title="最初の 1 秒を書き出し"
& "C:\path\to\Celesta\Celesta-export.exe" --from 0 --to 1 --react first-scene.tsx section.mp4
```

:::
::::

## 字幕ファイル

SRT や WebVTT の字幕ファイルは、`import` してから `parseSubtitles()` で読み込みます。結果は `{ from, durationInFrames, text }` の配列で、`Sequence` にそのまま渡せる形です。BOM、CRLF、複数行の字幕、`,` と `.` のどちらで区切ったミリ秒にも対応し、`<i>` などのタグは取り除きます。

```tsx title="SRT の字幕を描く"
import { Sequence, Subtitles, Text, parseSubtitles } from '@celesta.studio/react';
import captions from './captions.srt';

const cues = parseSubtitles(captions, { fps: 30 });

// コンポジションの中で
<Subtitles cues={cues} />
{cues.map((cue, index) => (
  <Sequence key={index} from={cue.from} durationInFrames={cue.durationInFrames}>
    <Text x={960} y={960} anchorX={0.5} anchorY={0.5}>{cue.text}</Text>
  </Sequence>
))}
```

`--subtitles` を付けると、動画と一緒に字幕ファイルを書き出します。拡張子が `.srt` なら SRT、`.vtt` なら WebVTT です。`<Subtitles>` で宣言した字幕と `<Dialogue>` の台詞が、表示されている間の時刻で入ります。`--from` / `--to` と組み合わせると、字幕の時刻も区間の開始を 0 として詰め、区間の外にはみ出す部分は切ります。

::::tabs[書き出しを行う OS]{default="macOS"}
:::tab[macOS]

```sh title="字幕ファイルも書き出す"
"/Applications/Celesta.app/Contents/MacOS/Celesta-export" --react first-scene.tsx output.mp4 --subtitles output.srt
```

:::
:::tab[Windows]

```sh title="字幕ファイルも書き出す"
& "C:\path\to\Celesta\Celesta-export.exe" --react first-scene.tsx output.mp4 --subtitles output.srt
```

:::
::::

時刻付きのデータを自分で書き出すときは `formatSubtitles(cues, { fps, format: 'srt' })` を使います。

## エンコーダーと設定を選ぶ

Celesta は動画を 3 つのメディアコンポーネントで書き出します。ファイルを書くマルチプレクサー（muxer）、映像エンコーダー、音声エンコーダーです。どれも `ffmpeg/libx264` のように `<プラグイン>/<コンポーネント>` という名前を持ちます。出力の拡張子でマルチプレクサーが決まり、マルチプレクサーの既定のコーデックでエンコーダーが決まります。

| 拡張子 | マルチプレクサー | 映像エンコーダー | 音声エンコーダー |
| --- | --- | --- | --- |
| `.mp4`、`.m4v` | `ffmpeg/mp4` | `ffmpeg/libx264`（H.264） | `ffmpeg/aac`（AAC） |
| `.mov` | `ffmpeg/mov` | `ffmpeg/prores`（ProRes 4444） | `ffmpeg/pcm`（24 bit PCM） |
| `.webm` | `ffmpeg/webm` | `ffmpeg/vp9`（アルファ付き VP9） | `ffmpeg/opus`（Opus、48 kHz） |

`--list-components` は、入っているコンポーネントを種類とプラグインとともに一覧にします。`--check` を加えると、各プラグインにそのコンポーネントがこのコンピューターで使えるかを尋ねます（プラグインを 1 回ずつ起動します）。`--describe <プラグイン/id>` は、ひとつのコンポーネントの設定を、キーごとに型、既定値、選択肢または範囲とともに表示します。どちらも `--json` で JSON を出力し、ソースも出力も取りません。

`--video-encoder` と `--audio-encoder` はマルチプレクサーの既定とは別のエンコーダーを、`--muxer` は拡張子とは別のマルチプレクサーを選びます。`--audio-encoder none` は音声のない動画を書き出します。`--video-option key=value` と `--audio-option key=value` は選んだエンコーダーの設定を指定し、値はその設定の型で読まれます。複数の設定は繰り返して指定します。エンコーダーにないキーや受け付けない値は、描画を始める前に失敗します。Celesta に同梱の FFmpeg プラグインには、このリリースでは次の設定があります（入っているコンポーネントの設定は `--describe` で確認できます）。

| コンポーネント | キー | 値（既定値） |
| --- | --- | --- |
| `ffmpeg/libx264` | `preset` | `ultrafast`、`superfast`、`veryfast`、`faster`、`fast`、`medium`、`slow`、`slower`、`veryslow`（`medium`） |
| | `rateControl` | 品質一定の `crf`、平均ビットレートの `vbr`（`crf`） |
| | `crf` | 0〜51、`rateControl=crf` のとき。小さいほど高画質・大容量（18） |
| | `bitrate` | ビット毎秒、`rateControl=vbr` のとき（8000000） |
| | `bitDepth` | `8`、または 10 bit の H.264（High 10）の `10`（`8`） |
| | `tune` | `film` や `animation` などの x264 の tune（なし） |
| `ffmpeg/prores` | `profile` | `4444`、またはアルファのない 422 HQ の `hq`（`4444`） |
| `ffmpeg/vp9` | `crf` | 0〜63（20） |
| | `alpha` | `true` または `false`（`true`） |
| `ffmpeg/aac`、`ffmpeg/opus` | `bitrate` | 32000〜512000 ビット毎秒（192000） |
| `ffmpeg/pcm` | `format` | `s16le`、`s24le`、`f32le`（`s24le`） |

マルチプレクサーは既定の設定のままです。MP4 と MOV は `faststart` 付きで書き出し、インデックスを先頭に置くので、ブラウザーはダウンロードが終わる前に再生を始められます。

`veryfast` などの速い x264 のプリセットは、画質をほぼ保ちながらエンコード時間を短縮しますが、ファイルは大きくなる傾向があります。`bitDepth=10` は暗いグラデーションをより滑らかに保てますが、再生できる環境は限られます。`--render-quality draft` は拡大した文字の再描画を省略して動きの確認を速くします。仕上がりの確認には既定の `final` を使ってください。エンコーダーが YUV を受け取り、描画がソフトウェアでないときは、Celesta が GPU で RGB から YUV に変換します。それ以外ではエンコーダーが変換します。

`--preset <名前>` は書き出しプリセット（これらの選択をまとめて保存したもの）を指定します。プリセットは今後のリリースで加わるため、今はどの名前もエラーコード `unknown_preset` で失敗します。x264 のプリセットは上の設定です。`--preset veryfast` は失敗し、そのヒントは `--video-option preset=veryfast` を使うよう伝えます。

::::tabs[書き出しを行う OS]{default="macOS"}
:::tab[macOS]

```sh title="速いプリセットで書き出す"
"/Applications/Celesta.app/Contents/MacOS/Celesta-export" --react --video-option preset=veryfast --video-option crf=18 first-scene.tsx draft.mp4
```

```sh title="入っているコンポーネントを一覧にする"
"/Applications/Celesta.app/Contents/MacOS/Celesta-export" --list-components --check
```

:::
:::tab[Windows]

```sh title="速いプリセットで書き出す"
& "C:\path\to\Celesta\Celesta-export.exe" --react --video-option preset=veryfast --video-option crf=18 first-scene.tsx draft.mp4
```

```sh title="入っているコンポーネントを一覧にする"
& "C:\path\to\Celesta\Celesta-export.exe" --list-components --check
```

:::
::::

MCP の `export_video` ツールと `@celesta.studio/exporter` は、同じ選択を `muxer`、`videoEncoder`（`{ "id": "ffmpeg/libx264", "options": { "preset": "veryfast", "crf": 18 } }`）、`audioEncoder`（同じ形、または `"none"`）で受け取ります。MCP の `list_components` と `describe_component` ツール、パッケージの `listComponents()` と `describeComponent()` で、コンポーネントを一覧にしたり、その設定を確認したりできます。

## 繰り返し再生する動画

Shorts や TikTok は動画を繰り返し再生します。最後のフレームが最初のフレームへつながる動画では、`--loop` を付けて音もつながるようにします。AAC のエンコーダーは無音から符号化を始めるので、付けないと最初のフレームのキックなどの立ち上がりが約 10 ms にじみ、繰り返すたびにそれが鳴ります。`--loop` を付けると、エンコーダーは先にサウンドトラックの末尾を符号化し、その分は MP4 の edit list で隠します。音声の長さは映像とちょうど同じままです。サウンドトラック自体はループしている必要があり、末尾からはみ出す響きは先頭に重ねておきます。`--loop` はどの動画にも付けられます。MP4 と MOV はその分を隠し、WebM のようにそれを隠せないマルチプレクサーでは `--loop` は何も変えません。`.mov` の PCM 音声にはエンコーダーの遅延がないので、付けなくてもつながります。

::::tabs[書き出しを行う OS]{default="macOS"}
:::tab[macOS]

```sh title="繰り返し再生する動画を書き出す"
"/Applications/Celesta.app/Contents/MacOS/Celesta-export" --react --loop first-scene.tsx loop.mp4
```

:::
:::tab[Windows]

```sh title="繰り返し再生する動画を書き出す"
& "C:\path\to\Celesta\Celesta-export.exe" --react --loop first-scene.tsx loop.mp4
```

:::
::::

## 配信・放送向けのラウドネス

配信サービスはどの動画もおおむね同じ音量で再生します。YouTube ではおよそ -14 LUFS で、大きすぎる動画は下げられ、小さすぎる動画は小さいまま再生されます。動画の書き出しでは毎回、サウンドトラック全体の統合ラウドネス（ITU-R BS.1770。FFmpeg の `ebur128` フィルターと同じ測り方）とトゥルーピークを測定します。コマンドラインでは `export complete` のあとに表示され、アプリでは書き出しの完了時に表示され、`--json` の結果にも含まれます。

`--loudness` を付けると、サウンドトラックを目標値に合わせます。値は -70 から -5 までの LUFS、`streaming`（-14）、`broadcast`（-23、EBU R128）のいずれかです。ミックス全体に 1 つのゲインを掛けるので、ミックスしたバランスは変わりません。ゲインによってトゥルーピークが `--true-peak`（既定は -1 dBTP、-9 から 0 まで）を超える箇所では、リミッターがピークをその値に抑えます。`--peak-handling lower` を付けると、代わりにいちばん大きいピークが上限に届いたところでゲインを止めます。サウンドトラックは目標より小さくなりますが、それ以外は手を加えません。このときは、どこまで大きくなったかを警告で伝えます。無音のサウンドトラックや 0.4 秒より短いものには統合ラウドネスがないため、そのまま書き出します。同じプロジェクトからは毎回同じ音声が書き出されます。

::::tabs[書き出しを行う OS]{default="macOS"}
:::tab[macOS]

```sh title="配信向けのラウドネスで書き出す"
"/Applications/Celesta.app/Contents/MacOS/Celesta-export" --react --loudness streaming first-scene.tsx output.mp4
```

:::
:::tab[Windows]

```sh title="配信向けのラウドネスで書き出す"
& "C:\path\to\Celesta\Celesta-export.exe" --react --loudness streaming first-scene.tsx output.mp4
```

:::
::::

ラウドネスは、エンコードする前のサウンドトラックで測定・調整します。`.mov` の PCM ではその値がそのまま保たれます。AAC（`.mp4`）と Opus（`.webm`）では、ラウドネスが 0.1 LU 程度、トゥルーピークが上下に 0.5 dB 程度動くため、既定の -1 dBTP がファイルでは -0.5 dBTP 程度になることがあります。余裕が必要なら `--true-peak -2` を指定します。アプリでは **Export…** の隣の **Loudness** で、配信向けか放送向けの目標値を -1 dBTP のリミッター付きで指定できます。それ以外の目標値や上限、`--peak-handling lower` を使うには、コマンドライン、MCP の `export_video` ツール、`@celesta.studio/exporter` を使います。

## 透過動画

動画の形式は拡張子で決まります。`.mp4` は上記のとおり H.264 映像と AAC 音声です。`.mov` は ProRes 4444（`yuva444p10le`）と 24 bit PCM 音声、`.webm` はアルファ付きの VP9（`yuva420p`、CRF 20）と 48 kHz の Opus 音声です。アプリでは保存ダイアログで拡張子を選びます。形式を選ぶ別のメニューはありません。コマンドラインでは、上記と同じコマンドの出力を `overlay.mov` や `overlay.webm` にします。アルファを残すかどうかはエンコーダーが決めます。ProRes 4444 と VP9 は、`--video-option profile=hq` や `--video-option alpha=false` を指定しない限りアルファを残し、H.264 は残しません。

何も描かれていない領域だけが透明になるため、フレーム全体を覆うレイヤー（全面の背景など）があると出力は不透明になります。アルファはストレート（非乗算済み）で、色は BT.709（リミテッドレンジ）に変換されてタグ付けされます。

Celesta は[メディアプラグイン](/ja/docs/plugins/#メディアプラグイン)でエンコードします。プラグインの FFmpeg のビルドに動画に必要なエンコーダーがない場合（libx264 のない LGPL ビルドなど）、書き出しはエラーコード `component_unavailable` で、その理由とともに失敗します。メディアプラグインがない場合は、その拡張子を書けるマルチプレクサーがないため、プラグインの入れ方のヒント付きで `unsupported_output` で失敗します。

## 静止画とコンタクトシート

--frame / --frames で、0 から始まるフレーム番号を指定します。複数指定すると check-000090.png のような名前で保存されます。--every N は --from/--to の範囲（省略時は全体）から N フレームごとと最後のフレームを選びます。--contact-sheet を加えると、番号付きの一覧画像を作れます。列数は --columns、各画像の幅は --tile-width で指定します。フレーム番号の直接指定は --every、--from、--to と併用できません。

画像の形式は拡張子で決まります。`.png` は可逆圧縮の PNG、`.jpg` と `.jpeg` は JPEG、`.webp` は非可逆圧縮の WebP です。JPEG と WebP はファイルが小さく、サムネイルに向きます。`--quality` で品質を 1〜100（既定値 90）の範囲で指定でき、PNG には指定できません。フレームは MP4 と同じ不透明な背景の上に描くため、透過チャンネルを持たない JPEG でも失われるものはありません。WebP は一辺 16383 ピクセルまでのため、それより大きいコンタクトシートは描画前にエラーになります。

::::tabs[書き出しを行う OS]{default="macOS"}
:::tab[macOS]

```sh title="指定したフレームを書き出す"
"/Applications/Celesta.app/Contents/MacOS/Celesta-export" --react --frames 0,90 first-scene.tsx check.png
```

```sh title="コンタクトシートを書き出す"
"/Applications/Celesta.app/Contents/MacOS/Celesta-export" --react --every 30 --contact-sheet --columns 4 --tile-width 320 first-scene.tsx sheet.png
```

```sh title="JPEG のサムネイルを書き出す"
"/Applications/Celesta.app/Contents/MacOS/Celesta-export" --react --frame 90 --quality 85 first-scene.tsx thumbnail.jpg
```

:::
:::tab[Windows]

```sh title="指定したフレームを書き出す"
& "C:\path\to\Celesta\Celesta-export.exe" --react --frames 0,90 first-scene.tsx check.png
```

```sh title="コンタクトシートを書き出す"
& "C:\path\to\Celesta\Celesta-export.exe" --react --every 30 --contact-sheet --columns 4 --tile-width 320 first-scene.tsx sheet.png
```

```sh title="JPEG のサムネイルを書き出す"
& "C:\path\to\Celesta\Celesta-export.exe" --react --frame 90 --quality 85 first-scene.tsx thumbnail.jpg
```

:::
::::

`--debug-guides` を加えると、React コンポジションの `<DebugOverlay>` と `<DebugBounds>` を、プレビューと同じように静止画やコンタクトシートに描きます。動画の書き出しには描かれません。

::::tabs[書き出しを行う OS]{default="macOS"}
:::tab[macOS]

```sh title="コンタクトシートでセーフエリアを確かめる"
"/Applications/Celesta.app/Contents/MacOS/Celesta-export" --react --every 30 --contact-sheet --debug-guides first-scene.tsx sheet.png
```

:::
:::tab[Windows]

```sh title="コンタクトシートでセーフエリアを確かめる"
& "C:\path\to\Celesta\Celesta-export.exe" --react --every 30 --contact-sheet --debug-guides first-scene.tsx sheet.png
```

:::
::::

## 描画に使う API を選ぶ

アプリと書き出しツールの両方で、--driver に auto（既定）、vulkan、dx12（Windows）、metal（macOS）、gl を指定できます。auto は wgpu の順序（Vulkan、Metal、DX12、OpenGL）で利用可能な描画アダプタを探します。明示した API は、そのビルドと実行環境の両方で利用できる必要があります。これは描画用 API の選択で、エンコードは引き続きメディアコンポーネントで行います。

::::tabs[書き出しを行う OS]{default="macOS"}
:::tab[macOS]

```sh title="描画 API を指定する"
"/Applications/Celesta.app/Contents/MacOS/Celesta-export" --react --driver auto first-scene.tsx output.mp4
```

:::
:::tab[Windows]

```sh title="描画 API を指定する"
& "C:\path\to\Celesta\Celesta-export.exe" --react --driver auto first-scene.tsx output.mp4
```

:::
::::

スクリプトや AI エージェント向けには `--json` を追加します。進捗は表示されず、完了時に JSON を 1 行だけ出力します。状態、生成ファイル、警告、読み込み後のサイズ・fps・フレーム数、失敗時のエラーコードとメッセージが含まれます。生成ファイルのうち動画には `"muxer"`（`"ffmpeg/mp4"` など）、`"video"` と `"audio"`（エンコーダーの `"component"` と書き出したコーデック文字列の `"codec"`、たとえば `"avc1.64001F"`。音声のない動画では `"audio"` は `null`）、`"alpha"`（`true` または `false`）、マルチプレクサーが書いたすべてのファイルの `"files"` が付きます。音声のある動画には `"loudness"`（`"integratedLufs"`、`"truePeakDbtp"`、`--loudness` を付けたときは掛けたゲインの `"gainDb"`）も付きます。コンポーネントに関するエラーコードは、`unsupported_output`（出力の拡張子を書けるマルチプレクサーがない）、`component_unavailable`（コンポーネントが入っていないか、ここでは使えない。理由付き）、`incompatible_components`（マルチプレクサーがエンコーダーのコーデックを受け付けない）、`invalid_option`（エンコーダーにない設定や受け付けない値）、`unknown_preset` です。引数が拒否された場合は、状態とエラーのみです。

ソースビルドを使う場合は、[開発者向け手順](/ja/docs/build-from-source/)に対応する Cargo コマンドがあります。

時刻は秒数、`MM:SS.mmm`、`HH:MM:SS.mmm` を指定できます。選んだ範囲を、自身の 00:00 から始まる新しい動画にします。

## GPU がない Linux

Linux で Vulkan を使う場合は --driver vulkan を指定します。CI やクラウドのコンテナなど GPU のない環境では、CPU で描画する Mesa のソフトウェア Vulkan ドライバ（lavapipe）をインストールしてください。[ソースからビルドしたツール](/ja/docs/build-from-source/)でも使えます。

```sh title="ソフトウェア Vulkan をインストール"
sudo apt-get install -y mesa-vulkan-drivers
```

ソフトウェア描画は GPU より遅く、4 コアでは 1080p をシーンによって約 10〜60 fps で描画します。全体の前に PNG で 1 フレーム確認してください。リポジトリのルートで実行します。

```sh title="1 フレームを書き出し"
cargo run -p celesta-exporter --release -- --react --driver vulkan packages/cli/examples/title.tsx --frame 0 frame.png
```

最小構成のコンテナにはフォントがないことがあり、少なくとも 1 つなければテキストは描けません。`fonts-dejavu-core`（日本語には `fonts-noto-cjk`）などをインストールするか、`<Font>` でファイルを読み込んでください。

ソフトウェア描画では、RGB から YUV への変換はエンコーダーが行います。そのほうが描画側で変換するより速いためです。`--render-quality draft` はタイミング確認には速いですが、ピクセル確認には不向きです。自分で Celesta をビルドせずコンテナから書き出すには、ルートで [Linux コンテナイメージ](https://github.com/mika-f/Celesta/tree/main/packaging/linux)を作り、`/work` にマウントするプロジェクトのフォルダから実行します。`--user` で出力の所有者を root ではなく自分にします。

```sh title="コンテナイメージをビルド"
docker build -f packaging/linux/Dockerfile -t celesta-exporter .
```

```sh title="コンテナで書き出し"
docker run --rm --user "$(id -u):$(id -g)" -v "$PWD:/work" celesta-exporter --react first-scene.tsx output.mp4
```

> [!NOTE]
> **書き出しの前に**
> MP4 は H.264 映像と AAC 音声です。MOV と WebM については「透過動画」を参照してください。幅・高さは 0 でない偶数にしてください。書き出し中はすべての参照メディアを利用可能にしておきます。
