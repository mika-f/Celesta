## ブラウザから

[ウェブエディタ](/ja/#playground)で TSX を編集するか開き、**MP4 に書き出す**を選びます。ソース、プレビュー、書き出しは Celesta の React シーン評価器を使います。**メディアを追加**でローカルファイルを名前で追加できます。H.264 の WebCodecs 対応が必要で、音声には AAC エンコードも必要です。プレビューは無音ですが MP4 には一定速度の音声クリップを含められます。文字の計測・自動調整（TextBox、fitText、useFitText、measureText、useTextMetrics）、PSD、プロジェクトのタイムライン、`prepare()`、外部インポート、音声の速度・音量アニメーション、ネイティブ描画にはアプリまたは CLI を使います。

## アプリから

**Export…** で保存先を選びます。拡張子を `.mp4` にすると H.264 の動画、`.mov` または `.webm` にすると透過動画（後述の「透過動画」）になります。それ以外の拡張子は `.mp4` になります。一部だけ書き出すには `I` と `O` で開始・終了を指定します。進捗はステータスバーに表示され、**Cancel export** で中止できます。

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

既存の出力ファイルを置き換えるには `--overwrite` を追加します。`--from` と `--to` で時間範囲を指定できます。

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
import { Sequence, Subtitles, Text, parseSubtitles } from '@celesta/react';
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

`--subtitles` を付けると、動画と一緒に字幕ファイルを書き出します。拡張子が `.srt` なら SRT、`.vtt` なら WebVTT です。`<Subtitles>` で宣言した字幕と `<Dialogue>` の台詞が、表示されている間の時刻で入ります。`--from` / `--to` と組み合わせると、字幕の時刻も区間の開始を 0 として詰め、区間の外にはみ出す部分は切ります。`--subtitles` を繰り返すと、SRT と WebVTT のように複数のファイルを一度に書き出せます。

複数の言語の字幕は、`<Subtitles>` ごとに `lang` を付け（`<Subtitles lang="ja" cues={japanese} />`）、パスに `{lang}` を入れます。`--subtitles 'output.{lang}.srt'` で `output.ja.srt`・`output.en.srt` のように言語ごとのファイルになります。言語のない `<Dialogue>` の台詞は、どの言語のファイルにも入ります。シェルが `{lang}` を書き換えないよう、パスは引用符で囲んでください。動画のファイルを省くと（`--react first-scene.tsx --subtitles output.srt`）、動画を描画せずに字幕ファイルだけを書き出します。

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

## 画質と書き出し速度

MP4 は libx264 を使い、既定では --preset medium、--crf 18 で書き出します。veryfast などの速いプリセットは、画質をほぼ保ちながらエンコード時間を短縮しますが、ファイルは大きくなる傾向があります。CRF は 0〜51 で、小さいほど高画質・大容量になります。--color-conversion は auto、gpu、encoder を指定できます。--render-quality draft は拡大した文字の再描画を省略して動きの確認を速くします。仕上がりの確認には既定の final を使ってください。--bit-depth 10 は 10 bit の H.264（High 10）で書き出し、暗いグラデーションをより滑らかに保てますが、再生できる環境は限られます。既定は 8 です。

::::tabs[書き出しを行う OS]{default="macOS"}
:::tab[macOS]

```sh title="速いプリセットで書き出す"
"/Applications/Celesta.app/Contents/MacOS/Celesta-export" --react --preset veryfast --crf 18 first-scene.tsx draft.mp4
```

:::
:::tab[Windows]

```sh title="速いプリセットで書き出す"
& "C:\path\to\Celesta\Celesta-export.exe" --react --preset veryfast --crf 18 first-scene.tsx draft.mp4
```

:::
::::

## 透過動画

動画の形式は拡張子で決まります。`.mp4` は上記のとおり H.264 映像と AAC 音声です。`.mov` は ProRes 4444（`yuva444p10le`）と 24 bit PCM 音声、`.webm` はアルファ付きの VP9（`yuva420p`、CRF 20）と 48 kHz の Opus 音声です。アプリでは保存ダイアログで拡張子を選びます。形式を選ぶ別のメニューはありません。コマンドラインでは、上記と同じコマンドの出力を `overlay.mov` や `overlay.webm` にします。`--output-format mp4|mov|webm` で形式を明示でき、拡張子と食い違うと使い方のエラーになります。

何も描かれていない領域だけが透明になるため、フレーム全体を覆うレイヤー（全面の背景など）があると出力は不透明になります。アルファはストレート（非乗算済み）で、色は BT.709（リミテッドレンジ）に変換されてタグ付けされます。`--preset`、`--crf`、`--bit-depth`、`--color-conversion` は MP4 専用で、`.mov` や `.webm` と併用すると使い方のエラーになります。

FFmpeg のビルドに選んだ形式のエンコーダーがない場合、書き出しはエラーコード `encoder_unavailable` で失敗します。

## PNG 画像とコンタクトシート

--frame / --frames で、0 から始まるフレーム番号を指定します。複数指定すると check-000090.png のような名前で保存されます。--every N は --from/--to の範囲（省略時は全体）から N フレームごとと最後のフレームを選びます。--contact-sheet を加えると、番号付きの一覧画像を作れます。列数は --columns、各画像の幅は --tile-width で指定します。フレーム番号の直接指定は --every、--from、--to と併用できません。

::::tabs[書き出しを行う OS]{default="macOS"}
:::tab[macOS]

```sh title="指定したフレームを書き出す"
"/Applications/Celesta.app/Contents/MacOS/Celesta-export" --react --frames 0,90 first-scene.tsx check.png
```

```sh title="コンタクトシートを書き出す"
"/Applications/Celesta.app/Contents/MacOS/Celesta-export" --react --every 30 --contact-sheet --columns 4 --tile-width 320 first-scene.tsx sheet.png
```

:::
:::tab[Windows]

```sh title="指定したフレームを書き出す"
& "C:\path\to\Celesta\Celesta-export.exe" --react --frames 0,90 first-scene.tsx check.png
```

```sh title="コンタクトシートを書き出す"
& "C:\path\to\Celesta\Celesta-export.exe" --react --every 30 --contact-sheet --columns 4 --tile-width 320 first-scene.tsx sheet.png
```

:::
::::

`--debug-guides` を加えると、React コンポジションの `<DebugOverlay>` と `<DebugBounds>` を、プレビューと同じように PNG フレームやコンタクトシートに描きます。動画の書き出しには描かれません。

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

アプリと書き出しツールの両方で、--driver に auto（既定）、vulkan、dx12（Windows）、metal（macOS）、gl を指定できます。auto は wgpu の順序（Vulkan、Metal、DX12、OpenGL）で利用可能な描画アダプタを探します。明示した API は、そのビルドと実行環境の両方で利用できる必要があります。これは描画用 API の選択で、MP4 のエンコードには引き続き libx264 を使います。

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

スクリプトや AI エージェント向けには `--json` を追加します。進捗は表示されず、完了時に JSON を 1 行だけ出力します。状態、生成ファイル、警告、読み込み後のサイズ・fps・フレーム数、失敗時のエラーコードとメッセージが含まれます。生成ファイルのうち動画には `"format"`（`mp4`、`mov`、`webm`）と `"alpha"`（`true` または `false`）が付きます。引数が拒否された場合は、状態とエラーのみです。

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

ソフトウェア描画で `--color-conversion auto` を使うと RGB から YUV への変換をエンコーダーに任せます。`--render-quality draft` はタイミング確認には速いですが、ピクセル確認には不向きです。自分で Celesta をビルドせずコンテナから書き出すには、ルートで [Linux コンテナイメージ](https://github.com/mika-f/Celesta/tree/main/packaging/linux)を作り、`/work` にマウントするプロジェクトのフォルダから実行します。`--user` で出力の所有者を root ではなく自分にします。

```sh title="コンテナイメージをビルド"
docker build -f packaging/linux/Dockerfile -t celesta-exporter .
```

```sh title="コンテナで書き出し"
docker run --rm --user "$(id -u):$(id -g)" -v "$PWD:/work" celesta-exporter --react first-scene.tsx output.mp4
```

> [!NOTE]
> **書き出しの前に**
> MP4 は H.264 映像と AAC 音声です。MOV と WebM については「透過動画」を参照してください。幅・高さは 0 でない偶数にしてください。書き出し中はすべての参照メディアを利用可能にしておきます。
