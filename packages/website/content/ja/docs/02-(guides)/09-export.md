## ブラウザから

[ウェブエディタ](/ja/#playground)で TSX を編集するか開き、**MP4 に書き出す**を選びます。ソース、プレビュー、書き出しは Celesta の React シーン評価器を使います。**メディアを追加**でローカルファイルを名前で追加できます。H.264 の WebCodecs 対応が必要で、音声には AAC エンコードも必要です。プレビューは無音ですが MP4 には一定速度の音声クリップを含められます。文字の計測・自動調整（TextBox、fitText、useFitText、measureText、useTextMetrics）、PSD、プロジェクトのタイムライン、`prepare()`、外部インポート、音声の速度・音量アニメーション、ネイティブ描画にはアプリまたは CLI を使います。

## アプリから

タイトルバーの **Export…** で書き出しダイアログを開きます。上から順に次の項目があります。

- **Preset**：Celesta の既定の設定を使う **None** に続いて、プロジェクトのプリセットが **This project**、自分のプリセットが **My presets** の下に並びます（後述の「書き出しプリセット」）。
- **Container**：ファイルを書くマルチプレクサーです。H.264 の動画なら **MP4 · ffmpeg**、透過動画なら **QuickTime · ffmpeg** か **WebM · ffmpeg** を選びます（後述の「透過動画」）。
- **Video encoder** とその設定：エンコーダーの分け方どおりにまとまって並びます。ほかの選択のときだけ効く設定は、その選択をするまで隠れています。使うことの少ない設定は **Advanced** の中にあり、最初は閉じています。
- **Audio encoder** とその設定：音声のない動画にするには **None** を選びます。
- **Range**：**Whole composition**（全体）か、タイムラインで `I` と `O` を押して指定した開始・終了です。**Clear** で全体に戻せます。
- **Loudness**：**Off**、**Streaming (-14 LUFS)**、**Broadcast (-23 LUFS)** のいずれかで、サウンドトラックをラウドネスの目標値に合わせます（後述の「配信・放送向けのラウドネス」）。選んだ値はアプリを終了するまで保たれます。

コンポーネントには名前とプラグインが表示され、プロジェクトに付属するプラグインのものには **(project)** が付きます。ダイアログを開くと、Celesta は各メディアプラグインに、どのコンポーネントがこのコンピューターで使えるかを尋ねます。答えが返るまでは **Checking…** と表示され、使えないものは一覧に残りますが選べず、理由が添えられます。まったく答えられなかったプラグインには、次にダイアログを開いたときにもう一度尋ねます。ダイアログは、プロジェクトに既定のプリセットがあればそれで、なければ前回書き出したときの選択で、それもなければ `.mp4` の既定の設定で開きます。読めないプリセットファイルがあると、**Preset** の下にそのファイルと問題点が表示され、そのプリセットは一覧に出ません。

**Container** を変えると、ダイアログで選んだエンコーダーや、選んだプリセットが指定しているエンコーダーは、新しいコンテナーが受け付ければ設定ごと残ります。それ以外のエンコーダーは新しいコンテナーの既定に替わるので、**MP4** から **QuickTime** に変えると、`.mov` に書き出すときと同じく ProRes 4444 と PCM 音声になります。**None** にした音声はオフのままです。IVF のように音声を受け付けないコンテナーのせいで外れただけの音声は、次に音声を受け付けるコンテナーを選ぶと戻ります。

ダイアログの **Export…** で保存先を選びます。保存パネルにはコンテナーの最初の拡張子が入っており、コンテナーが書かない拡張子を付けた名前はその拡張子に置き換わります。**Cancel** や Escape でダイアログを閉じても選んだ内容は残り、アプリを終了するかプラグインを再読み込みするまで、次に開いたときもそのままです。**Export…** は選んだ内容を自分のプリセットファイルにも記録します（後述の「書き出しプリセット」）。そのファイルに書き込めなかったときは **Problems** タブの **Export presets** に表示され、書き出しはそのまま進みます。進捗はステータスバーに表示され、タイトルバーの **Cancel export** で中止できます。応答を待っているプラグインがあれば、それも止められます。書き出しが終わると、完了の通知にサウンドトラックのラウドネスが表示されます。書き出し中に出た警告とお知らせ（プレビューしていないフレームのフォントのフォールバックやプラグインのお知らせなど）は、**Problems** タブにひとつの項目として記録されます。それぞれ一度だけ、出た順に、コマンドラインの出力と同じ形で並びます。

**Save as preset…** は、ダイアログで選んだ内容に名前を付けて、**This project** か **My presets** に保存します。まだ保存していないプロジェクトは、先に保存する必要があります。保存先に同じ名前のプリセットがあるとボタンは **Replace** になり、そのプリセットを置き換えます。名前の欄で Enter を押しても保存できます。**Export…** はいつも書き出しです。保存したプリセットは **Preset** で選んだときと同じく選ばれた状態になるので、そのあと **Container** を変えてもそのエンコーダーが残ります。**Export…** と同じく、確認中や設定が正しくないあいだは保存できず、その理由が表示されます。プリセットを選ぶと、そのコンテナー、エンコーダー、設定が入ります。そのあとでどれかを変えると、**Preset** は **None** に戻ります。古いバージョンのプラグインで保存したプリセットには、プラグインにもうない設定や、もう受け付けない値、なくなったコンポーネントが含まれていることがあります。ダイアログはその設定を外すか、設定の既定値か既定のコンポーネントを使い、変えた内容をそれぞれ **Preset** の下に表示します。前回書き出したときの選択も同じように確かめ、その警告では「your last export」と呼びます。

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

`--list-components` は、入っているコンポーネントを種類とプラグインとともに一覧にします。Celesta に組み込みの画像コンポーネント（`builtin/png` など。`built-in` と表示されます）も含まれ、これらは静止画だけを書き出します（後述の「静止画とコンタクトシート」）。`--check` を加えると、各プラグインにそのコンポーネントがこのコンピューターで使えるかを尋ねます（プラグインを 1 回ずつ起動します）。`--describe <プラグイン/id>` は、ひとつのコンポーネントの設定を、キーごとに型、既定値、選択肢または範囲とともに表示します。どちらも `--json` で JSON を出力し、ソースも出力も取りません。

`--video-encoder` と `--audio-encoder` はマルチプレクサーの既定とは別のエンコーダーを、`--muxer` は拡張子とは別のマルチプレクサーを選びます。そのマルチプレクサーが出力の拡張子を挙げていなくてもファイルは書き出され、`ffmpeg/webm does not write .mp4 files; the file is written as WebM anyway` のような警告が出ます。`--audio-encoder none` は音声のない動画を書き出します。`--audio-option` とは一緒に指定できません。マルチプレクサーが音声を受け付けず、音声エンコーダーも指定していないときも、`--audio-option` は指定できません。`--video-option key=value` と `--audio-option key=value` は選んだエンコーダーの設定を指定し、値はその設定の型で読まれます。複数の設定は繰り返して指定します。エンコーダーにないキーや受け付けない値は、描画を始める前に失敗します。Celesta に同梱の FFmpeg プラグインには、このリリースでは次の設定があります（入っているコンポーネントの設定は `--describe` で確認できます）。

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

すべての動画の書き出しは、リミテッドレンジの BT.709 です。変換は GPU で行い、エンコーダーが RGB を受け取るときはエンコーダーが行います。ファイルにはそのことを示すタグ（H.264 の VUI、MP4 と MOV の `colr`、WebM の `Colour`）が付くので、プレーヤーは描画どおりの色で表示します。以前のリリースは、タグのない MP4 を BT.601 の係数で書き出していたため、HD を BT.709 とみなすプレーヤーでは色がわずかにずれて見えました。

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

MCP の `export_video` ツールと `@celesta.studio/exporter` は、同じ選択を `muxer`、`videoEncoder`（`{ "id": "ffmpeg/libx264", "options": { "preset": "veryfast", "crf": 18 } }`）、`audioEncoder`（同じ形、または `"none"`）で受け取り、書き出しプリセットを `preset` と `presetFile` で受け取ります（後述の「書き出しプリセット」）。MCP は各オプションの型を確かめるので、`crf` は数値で渡します。パッケージはオプションを `--video-option` と `--audio-option` のテキストとして書き出しツールに渡し、コマンドラインと同じく各設定の型で読むので、`"18"` でも受け付けます。MCP の `list_components` と `describe_component` ツール、パッケージの `listComponents()` と `describeComponent()` で、コンポーネントを一覧にしたり、その設定を確認したりできます。

## 書き出しプリセット

書き出しプリセットは、コンテナー、エンコーダー、その設定の組み合わせに名前を付けたものです。プリセットは `export-presets.json` という名前のファイルに、2 か所で保存されます。

- 自分のプリセットは、Celesta の設定フォルダーの `settings.json` の隣にあります。macOS では `~/Library/Application Support/Celesta`、Windows では `%APPDATA%\Celesta`、それ以外では `$XDG_CONFIG_HOME/celesta`（既定では `~/.config/celesta`）です。
- プロジェクトのプリセットは、プロジェクトのルート（`celesta.config.json` のあるフォルダー、なければコンポジションのあるフォルダー）にあります。プロジェクトのリポジトリと一緒に運ばれるので、CI での書き出しにも使われます。

どちらもアプリの **Save as preset…** で書き込めるほか、手で書くこともできます。書き直したファイルはアクセス権をそのまま保ち（新しいファイルは macOS と Linux では誰でも読める `0644` になります）、シンボリックリンクのファイルはリンク先に書き込まれます。

```json title="export-presets.json"
{
  "version": 1,
  "presets": [
    { "name": "YouTube 1080p",
      "muxer": "ffmpeg/mp4",
      "video": { "component": "ffmpeg/libx264", "options": { "crf": 18, "preset": "slow" } },
      "audio": { "component": "ffmpeg/aac", "options": { "bitrate": 192000 } } },
    { "name": "Silent", "muxer": "ffmpeg/mp4", "audio": "none" }
  ],
  "default": "YouTube 1080p"
}
```

コンポーネントは `<プラグイン>/<コンポーネント>` で指定し、`"audio": "none"` は音声を入れません。プリセットで省いた部分は既定のままです。`muxer` がなければ出力の拡張子で決まり、`component` がなければマルチプレクサーの既定のエンコーダーが、プリセットに `options` があればそれとともに使われます（下のように自分でエンコーダーを指定したときを除きます）。`options` には既定値と違う設定だけを書けば十分です。**Save as preset…** もそれだけを保存するので、変えていない設定はプラグインがのちに既定値を変えるとそれに従います。`"default"` を持てるのはプロジェクトのファイルだけです。自分のファイルには、アプリで前回書き出したときの選択も `"last"` として残ります。`version` は `1` です。

`--preset <名前>` でプリセットを使って書き出します。名前は `--preset-file <パス>` で指定したファイル、プロジェクトのファイル、自分のファイルの順に探し、最初に見つかったものを使います。プロジェクトのプリセットが自分のプリセットと同じ名前のときは、自分のほうが隠れることを警告します。どのファイルにもない名前はエラーコード `unknown_preset` で失敗します。x264 のプリセットは書き出しプリセットではなく上の設定です。`--preset veryfast` は失敗し、そのヒントは `--video-option preset=veryfast` を使うよう伝えます。`--json` を付けないときは、エラーのヒントはメッセージのあとの `hint:` の行に表示されます。`--preset-file` のファイルがないか、プリセットファイルでないときは、引数の誤りとして拒否されます。

ほかのフラグはプリセットより優先されます。`--muxer`、`--video-encoder`、`--audio-encoder` はプリセットのコンポーネントを置き換え、`--video-option` と `--audio-option` は指定したキーの値を置き換えます。プリセットの設定は、保存したときのエンコーダーにだけ使われます。プリセットがエンコーダーを指定しているときは、`--video-encoder`（または `--audio-encoder`）で同じエンコーダーを指定すれば設定は残り、別のエンコーダーを指定すれば使われません。プリセットがエンコーダーを指定していないときは、自分でもエンコーダーを指定しないあいだだけ、マルチプレクサーの既定のエンコーダーに設定が使われます。既定と同じエンコーダーであっても、指定すると使われません。プリセットのマルチプレクサーが出力の拡張子を挙げていないときも、`--muxer` と同じ警告を出してそのまま書き出します。

動画の書き出しで `--preset` も、`--muxer`、`--video-encoder`、`--audio-encoder` のどれも指定しないときは、プロジェクトのファイルに `default` があればそのプリセットを使います。`--audio-encoder none` もこのフラグに数えます。一方、`--video-option` と `--audio-option` だけなら既定のプリセットの上に重ねて使われます。`default` の名前のプリセットがどのファイルにもないときは `unknown_preset` で失敗します。読めない、JSON として正しくない、`version` が違うプリセットファイルは `invalid_presets` で失敗しますが、失敗するのは、そのファイルからプリセットを探す必要がある書き出しだけです。自分のファイルが壊れていても、既定のプリセットのないプロジェクトをプリセットを指定せずに書き出すときや、プリセット（または既定のプリセット）がプロジェクトのファイルで見つかるときは影響しません。後者では、同じ名前のプリセットがあるか自分のファイルを確かめられなかったことを警告して、そのまま書き出します。

古いバージョンのプラグインで保存したプリセットには、エンコーダーにもうない設定、もう受け付けない値、なくなったかここでは使えないコンポーネントが含まれていることがあります。そのプリセットでも書き出しは止まりません。その設定を外すか、設定の既定値か既定のコンポーネント（プリセットのそのコンポーネント向けの設定は使わずに）を使い、それぞれをコマンドラインと `--json` の結果の `warnings` で警告します。`--video-option` や `--audio-option` で自分で指定した値は、これまでどおり `invalid_option` で失敗します。ただし、音声を入れないプリセット（`"audio": "none"` で、`--audio-encoder` も指定しないとき）での `--audio-option` は、プリセットが音声を入れないことを警告して無視します。

::::tabs[書き出しを行う OS]{default="macOS"}
:::tab[macOS]

```sh title="プリセットで書き出す"
"/Applications/Celesta.app/Contents/MacOS/Celesta-export" --react --preset "YouTube 1080p" first-scene.tsx output.mp4
```

:::
:::tab[Windows]

```sh title="プリセットで書き出す"
& "C:\path\to\Celesta\Celesta-export.exe" --react --preset "YouTube 1080p" first-scene.tsx output.mp4
```

:::
::::

MCP の `export_video` ツールと `@celesta.studio/exporter` は `preset` と `presetFile` を受け取り、同じ順に探します。プロジェクトの既定のプリセットも同じ場合に使い、結果には同じ警告が入ります。

## macOS の AVFoundation コンポーネント

macOS のアプリには `avfoundation` メディアプラグインも入っています。Apple の VideoToolbox と AudioToolbox で（Mac にメディアエンジンがあればそれを使って）エンコードし、AVFoundation で MP4 と MOV のファイルを書きます。既定は FFmpeg のままです。これらのコンポーネントは、`--muxer`、`--video-encoder`、`--audio-encoder` や書き出しプリセット（`"muxer": "avfoundation/mp4"`、`"video": { "component": "avfoundation/hevc" }`）で名前を指定したときだけ使われます。FFmpeg のコンポーネントと組み合わせることもでき、`--video-encoder avfoundation/hevc` だけを指定すると、VideoToolbox の HEVC を `ffmpeg/mp4` に `ffmpeg/aac` と一緒に書きます。このプラグインは macOS 専用で、Windows と Linux にはありません。

| コンポーネント | 種類 | 書き出すもの |
| --- | --- | --- |
| `avfoundation/h264` | 映像エンコーダー | H.264 |
| `avfoundation/hevc` | 映像エンコーダー | HEVC（8 bit または 10 bit） |
| `avfoundation/prores` | 映像エンコーダー | アルファ付きの ProRes 4444、または 422 HQ |
| `avfoundation/aac` | 音声エンコーダー | 固定ビットレートの AAC |
| `avfoundation/mp4` | マルチプレクサー | `.mp4` と `.m4v`。H.264 または HEVC と、AAC。既定は `avfoundation/h264` と `avfoundation/aac` |
| `avfoundation/mov` | マルチプレクサー | `.mov`。H.264、HEVC、ProRes のいずれかと、AAC または PCM。既定は `avfoundation/prores`（4444）と `avfoundation/aac` |

Apple M4 では、VideoToolbox の H.264 と HEVC は、4K の作品を既定の設定の x264 の約 3 倍の速さで書き出しました。それぞれの既定の設定では、H.264 のファイルは x264 より約 10〜20% 大きく画質はわずかに低く、HEVC のファイルはほぼ同じ画質で約 15〜30% 小さくなりました。ただし、HEVC は H.264 より再生できる機器が限られます。

このリリースでの設定は次のとおりです（`--describe avfoundation/hevc` でも確認できます）。

| コンポーネント | キー | 値（既定値） |
| --- | --- | --- |
| `avfoundation/h264` | `profile` | `high`、`main`、`baseline`（`high`） |
| | `rateControl` | 品質一定の `quality`、平均ビットレートの `bitrate`（`quality`） |
| | `quality` | 0〜100、`rateControl=quality` のとき。大きいほど高画質・大容量（70） |
| | `bitrate` | ビット毎秒、100000 以上、`rateControl=bitrate` のとき（8000000） |
| | `keyframeInterval` | 1〜1000、キーフレームの間隔の最大フレーム数（250） |
| | `bFrames` | `true` または `false`。フレームを並べ替えると圧縮率が上がります。デコードできないプレーヤー向けには無効にします（`true`）。Baseline では常に使いません |
| `avfoundation/hevc` | `rateControl`、`quality`、`bitrate`、`keyframeInterval`、`bFrames` | `avfoundation/h264` と同じ |
| | `bitDepth` | Main の `8`、または Main 10 の `10`（`8`） |
| `avfoundation/prores` | `profile` | `4444`、またはアルファのない 422 HQ の `hq`（`4444`） |
| `avfoundation/aac` | `bitrate` | 32000〜320000 ビット毎秒（192000） |

AudioToolbox が受け付けるビットレートは、サンプリングレートとチャンネル数によってこの範囲より狭くなります。44.1 kHz と 48 kHz では、モノラルが 32000〜256000、ステレオが 64000〜320000 で、それより低いレートではさらに狭くなります。範囲外のビットレートは、音声をミックスし終えてエンコーダーを開くときにエラーコード `invalid_option` で失敗し、メッセージに範囲が示されます。既定値の 192000 は、32 kHz 未満のステレオと 44.1 kHz 未満のモノラルでは高すぎるため、そのレートでミックスする音声には低い `bitrate` を指定します。マルチプレクサーは FFmpeg のものと同じく既定の設定（`faststart` など）のままです。

`avfoundation/mov` は、PCM を書く `ffmpeg/mov` と違い、既定で AAC を書きます。PCM にするには `--audio-encoder ffmpeg/pcm` を指定します。`--loop` はどちらのマルチプレクサーでも使えます。先頭に入れた音声と AAC 自体の遅延は隠されるので、FFmpeg を使うプレーヤーでも Apple のプレーヤーでも、同じサンプルから音が鳴ります。色は FFmpeg と同じく、リミテッドレンジの BT.709 で、タグも付きます。Mac の VideoToolbox にそのコーデックのエンコーダーがないときは、コンポーネントはこの Mac では使えないと答え（`VideoToolbox has no HEVC encoder on this Mac`）、指定するとエラーコード `component_unavailable` で失敗します。どれが使えるかは `--list-components --check` で確認できます。

このプラグインには、MP4、MOV、M4A のファイルを読むデコーダー `avfoundation/avfoundation` もあります。FFmpeg のデコーダーが先に読むので、これが読むのは FFmpeg が読めないファイルだけです（`ffmpeg` プラグインをオフにしたときは、これらのファイルをすべて読みます）。色は AVFoundation 自身の変換で BT.709 にするため、HDR の動画は FFmpeg のデコーダーのようなトーンマッピングを通らず、見え方が異なることがあります。

```sh title="VideoToolbox で HEVC を書き出す"
"/Applications/Celesta.app/Contents/MacOS/Celesta-export" --react --muxer avfoundation/mp4 --video-encoder avfoundation/hevc --video-option quality=80 --audio-option bitrate=256000 first-scene.tsx output.mp4
```

MCP の `export_video` ツールと `@celesta.studio/exporter` も同じ名前を受け取ります。`"muxer": "avfoundation/mp4"`、`"videoEncoder": { "id": "avfoundation/hevc", "options": { "quality": 80 } }` のように指定します。

## Windows の Media Foundation コンポーネント

Windows のアプリには `mediafoundation` メディアプラグインも入っています。Windows 自身の Media Foundation で（Windows が GPU の動画エンコーダーを使えるときはそれを使って）H.264、HEVC、AAC をエンコードします。マルチプレクサーは持たず、エンコーダーはほかのプラグインのコンテナーに書きます。コンテナーはいつもどおり出力の拡張子で決まり（`.mp4` なら `ffmpeg/mp4`、`.mov` なら `ffmpeg/mov`）、`--muxer` でも指定できます。既定は FFmpeg のままです。これらのコンポーネントは、`--video-encoder` と `--audio-encoder` や書き出しプリセット（`"video": { "component": "mediafoundation/hevc" }`）で名前を指定したときだけ使われます。`--video-encoder mediafoundation/h264` だけを指定すると、Media Foundation の H.264 を `ffmpeg/mp4` に `ffmpeg/aac` と一緒に書きます。アプリの書き出しダイアログでも、映像と音声のエンコーダーの中に表示されます。このプラグインは Windows 専用で、macOS と Linux にはありません。

| コンポーネント | 種類 | 書き出すもの |
| --- | --- | --- |
| `mediafoundation/h264` | 映像エンコーダー | H.264 |
| `mediafoundation/hevc` | 映像エンコーダー | HEVC（8 bit または 10 bit） |
| `mediafoundation/aac` | 音声エンコーダー | 44.1 kHz または 48 kHz の AAC |

NVIDIA GeForce RTX 4070 では、GPU の H.264 と HEVC のエンコーダーは、4K の作品を既定の設定の x264 の約 2 倍の速さで書き出しました。それぞれの既定の設定では、ファイルは x264 より約 15% 小さく画質はわずかに低く、1080p ではほぼ同じ大きさでした。Microsoft のソフトウェアエンコーダー（`hardware=false`）は GPU のものより時間がかかり、ファイルは x264 より 10〜54% 大きくなりました。

このリリースでの設定は次のとおりです（`--describe mediafoundation/hevc` でも確認できます）。

| コンポーネント | キー | 値（既定値） |
| --- | --- | --- |
| `mediafoundation/h264` | `profile` | `high`、`main`、`baseline`（`high`） |
| | `rateControl` | 品質一定の `quality`、平均ビットレートの `bitrate`（`quality`） |
| | `quality` | 0〜100、`rateControl=quality` のとき。大きいほど高画質・大容量（70） |
| | `bitrate` | ビット毎秒、100000 以上、`rateControl=bitrate` のとき（8000000） |
| | `keyframeInterval` | 1〜1000、キーフレームの間隔の最大フレーム数（250） |
| | `bFrames` | `true` または `false`。フレームを並べ替えると圧縮率が上がります。デコードできないプレーヤー向けには無効にします（`true`）。Baseline では常に使いません |
| | `hardware` | Windows が GPU のエンコーダーを使えるときにそれを使う `true`、または Microsoft のソフトウェアエンコーダーを使う `false`（`true`） |
| `mediafoundation/hevc` | `rateControl`、`quality`、`bitrate`、`keyframeInterval`、`bFrames`、`hardware` | `mediafoundation/h264` と同じ |
| | `bitDepth` | Main の `8`、または Main 10 の `10`（`8`） |
| `mediafoundation/aac` | `bitrate` | `96000`、`128000`、`160000`、`192000` ビット毎秒のいずれか（`192000`） |

`bFrames` はフレームの並べ替えを求める設定で、NVIDIA のものなど並べ替えられないエンコーダーは、並べ替えずに映像を書きます。GPU のエンコーダーが指定した `rateControl` を受け付けないときは、Windows が挙げる次のエンコーダーを使い、最後は Microsoft のソフトウェアエンコーダーになります。Windows のソフトウェアの HEVC エンコーダーは Microsoft Store の HEVC ビデオ拡張機能に含まれていて 8 bit のみなので、10 bit の HEVC には GPU のエンコーダーが必要です。`bitDepth=10` と `hardware=false` を指定すると、エラーコード `component_unavailable` で失敗します。AAC は 16 bit のサンプルから、モノラルまたはステレオ、44.1 kHz または 48 kHz でエンコードします。ほかのレートでミックスする音声は 48 kHz でエンコードします。

`--loop` はこれらのエンコーダーでも使えます。先頭に入れた音声と、`mediafoundation/aac` が最初に入れる短い無音は、FFmpeg の AAC の遅延と同じく MP4 の edit list で隠します。色は FFmpeg と同じく、リミテッドレンジの BT.709 で、タグも付きます。Microsoft のソフトウェアエンコーダーは H.264 と HEVC のストリーム自体にこのタグを書かないので、プラグインが書き加えます。Windows にそのコーデックのエンコーダーがないときは、コンポーネントはこのコンピューターでは使えないと答え（`Media Foundation has no HEVC encoder on this system; install a GPU driver with one or the HEVC Video Extensions`）、指定するとエラーコード `component_unavailable` で失敗します。どれが使えるかは `--list-components --check` で確認できます。Media Foundation の機能を入れていない Windows Server や、Media Feature Pack を入れていない N エディションの Windows では、どれも使えません。

このプラグインには、MP4、MOV、M4A のファイルを読むデコーダー `mediafoundation/mediafoundation` もあります。FFmpeg のデコーダーが先に読むので、これが読むのは FFmpeg が読めないファイルだけです（`ffmpeg` プラグインをオフにしたときは、これらのファイルをすべて読みます）。色は Media Foundation 自身の変換によるため、HDR の動画は FFmpeg のデコーダーのようなトーンマッピングを通らず、見え方が異なることがあります。また、アルファは捨てます。

```sh title="Media Foundation で HEVC と AAC を書き出す"
& "C:\path\to\Celesta\Celesta-export.exe" --react --video-encoder mediafoundation/hevc --video-option quality=80 --audio-encoder mediafoundation/aac --audio-option bitrate=160000 first-scene.tsx output.mp4
```

MCP の `export_video` ツールと `@celesta.studio/exporter` も同じ名前を受け取ります。`"videoEncoder": { "id": "mediafoundation/hevc", "options": { "quality": 80 } }`、`"audioEncoder": { "id": "mediafoundation/aac", "options": { "bitrate": "160000" } }` のように指定します。AAC の `bitrate` は選択肢なので、MCP では文字列で渡します。

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

ラウドネスは、エンコードする前のサウンドトラックで測定・調整します。`.mov` の PCM ではその値がそのまま保たれます。AAC（`.mp4`）と Opus（`.webm`）では、ラウドネスが 0.1 LU 程度、トゥルーピークが上下に 0.5 dB 程度動くため、既定の -1 dBTP がファイルでは -0.5 dBTP 程度になることがあります。余裕が必要なら `--true-peak -2` を指定します。アプリでは書き出しダイアログの **Loudness** で、配信向けか放送向けの目標値を -1 dBTP のリミッター付きで指定できます。それ以外の目標値や上限、`--peak-handling lower` を使うには、コマンドライン、MCP の `export_video` ツール、`@celesta.studio/exporter` を使います。

## 透過動画

動画の形式は拡張子で決まります。`.mp4` は上記のとおり H.264 映像と AAC 音声です。`.mov` は ProRes 4444（`yuva444p10le`）と 24 bit PCM 音声、`.webm` はアルファ付きの VP9（`yuva420p`、CRF 20）と 48 kHz の Opus 音声です。アプリでは書き出しダイアログの **Container** で **QuickTime** か **WebM** を選びます。コマンドラインでは、上記と同じコマンドの出力を `overlay.mov` や `overlay.webm` にします。アルファを残すかどうかはエンコーダーが決めます。ProRes 4444 と VP9 は、`--video-option profile=hq` や `--video-option alpha=false` を指定しない限りアルファを残し、H.264 は残しません。

何も描かれていない領域だけが透明になるため、フレーム全体を覆うレイヤー（全面の背景など）があると出力は不透明になります。アルファはストレート（非乗算済み）で、色は BT.709（リミテッドレンジ）に変換されてタグ付けされます。

Celesta は[メディアプラグイン](/ja/docs/plugins/#メディアプラグイン)でエンコードします。プラグインの FFmpeg のビルドに動画に必要なエンコーダーがない場合（libx264 のない LGPL ビルドなど）、書き出しはエラーコード `component_unavailable` で、その理由とともに失敗します。メディアプラグインがない場合は、その拡張子を書けるマルチプレクサーがないため、プラグインの入れ方のヒント付きで `unsupported_output` で失敗します。

## 静止画とコンタクトシート

--frame / --frames で、0 から始まるフレーム番号を指定します。複数指定すると check-000090.png のような名前で保存されます。--every N は --from/--to の範囲（省略時は全体）から N フレームごとと最後のフレームを選びます。--contact-sheet を加えると、番号付きの一覧画像を作れます。列数は --columns、各画像の幅は --tile-width で指定します。フレーム番号の直接指定は --every、--from、--to と併用できません。

画像の形式は拡張子で決まります。書き出すのは Celesta に組み込みの画像コンポーネントなので、静止画にはメディアプラグインは要りません。`.png` は可逆圧縮の PNG（`builtin/png`）、`.jpg` と `.jpeg` は JPEG（`builtin/jpeg`）、`.webp` は WebP（`builtin/webp`）で、後述の `lossless=true` を指定しない限り非可逆圧縮です。JPEG と WebP はファイルが小さく、サムネイルに向きます。`--quality` で品質を 1〜100（既定値 90）の範囲で指定でき、PNG には指定できません。既定では、フレームは MP4 と同じ不透明な背景の上に描くため、透過チャンネルを持たない JPEG でも失われるものはありません。PNG と WebP は、後述の `transparent=true` で透過を残せます。コンタクトシートは一辺 16384 ピクセルまでで、それより大きいものは描画前に `invalid_selection` で失敗します。WebP は一辺 16383 ピクセルまでのため、幅か高さが 16384 ピクセルの WebP のシートも、描画前に `invalid_option` で失敗します。

`--video-option key=value` で、動画のエンコーダーと同じように画像エンコーダーの設定を指定できます。設定は `--describe builtin/png` などで確認できます。

| コンポーネント | キー | 値（既定値） |
| --- | --- | --- |
| `builtin/png` | `compression` | `none`、`fastest`、`fast`、`balanced`、`high`。どれも可逆圧縮で、小さいファイルほど書き出しに時間がかかります（`balanced`） |
| | `depth` | `8`、または 16 bit の PNG の `16`（`8`）。コンタクトシートは 8 bit のみです |
| | `transparent` | `true` で背景を透明のままにします（`false`） |
| `builtin/jpeg` | `quality` | 1〜100、`--quality` と同じです（90） |
| `builtin/webp` | `lossless` | `true` で可逆圧縮の WebP にします（`false`） |
| | `quality` | 1〜100、`--quality` と同じです。可逆圧縮でないとき（90） |
| | `transparent` | PNG と同じです（`false`） |

品質は `--quality` か `--video-option quality=` のどちらか一方で指定します。`transparent=true` にすると、透過動画と同じように、何も描かれていない領域が透明のまま残ります。アルファはストレート（非乗算済み）で、フレーム全体を覆うレイヤーがあると画像は不透明になります。JPEG には透過チャンネルがないため、この設定はありません。コンタクトシートでは透明な領域がグレーの市松模様の上に表示され、シート自体は不透明です。

メディアプラグインは、ほかの画像形式や、これらの形式を書く独自のコンポーネントを追加できます。Celesta がそれを使うのは、`--muxer` と `--video-encoder` で名前を指定したときだけです。`builtin/image` のように 1 枚の画像を書くマルチプレクサーは 1 フレームかコンタクトシートを、複数のフレームは `builtin/imageSequence` のような連番用のマルチプレクサーを使います。静止画には書き出しプリセットは使えません。`--preset`、`--preset-file`、プロジェクトの既定のプリセット、音声のオプションは動画にだけ適用されます。複数のフレームは 1 枚ずつ書き出すため、途中のフレームで失敗したり書き出しをキャンセルしたりしても、それまでに書き出したファイルは残ります。1 枚の画像は完成してから書き出すため、キャンセルすると何も残りません。MCP の `render_frames` と `contact_sheet` ツール、`@celesta.studio/exporter` の `exportStills` は、動画の書き出しと同じく `muxer` と `videoEncoder`（`{ "id": "builtin/webp", "options": { "lossless": true } }`）を受け取ります。`depth` のような選択式の設定は、文字列（`"16"`）で指定します。

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

```sh title="背景が透明な PNG を書き出す"
"/Applications/Celesta.app/Contents/MacOS/Celesta-export" --react --frame 90 --video-option transparent=true first-scene.tsx overlay.png
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

```sh title="背景が透明な PNG を書き出す"
& "C:\path\to\Celesta\Celesta-export.exe" --react --frame 90 --video-option transparent=true first-scene.tsx overlay.png
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

スクリプトや AI エージェント向けには `--json` を追加します。進捗は表示されず、完了時に JSON を 1 行だけ出力します。状態、生成ファイル、警告、読み込み後のサイズ・fps・フレーム数、失敗時のエラーコードとメッセージが含まれます。生成ファイルのうち動画には `"muxer"`（`"ffmpeg/mp4"` など）、`"video"` と `"audio"`（エンコーダーの `"component"` と書き出したコーデック文字列の `"codec"`、たとえば `"avc1.64001F"`。音声のない動画では `"audio"` は `null`）、`"alpha"`（`true` または `false`）、マルチプレクサーが書いたすべてのファイルの `"files"` が付きます。音声のある動画には `"loudness"`（`"integratedLufs"`、`"truePeakDbtp"`、`--loudness` を付けたときは掛けたゲインの `"gainDb"`）も付きます。コンポーネントに関するエラーコードは、`unsupported_output`（出力の拡張子を書けるマルチプレクサーがない）、`component_unavailable`（コンポーネントが入っていないか、ここでは使えない。理由付き）、`incompatible_components`（マルチプレクサーがエンコーダーのコーデックを受け付けない）、`invalid_option`（エンコーダーにない設定や受け付けない値）、`unknown_preset`（その名前のプリセットがどのプリセットファイルにもない）、`invalid_presets`（プリセットファイルを読めないか、プリセットファイルではない）です。引数が拒否された場合は、状態とエラーのみです。

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
