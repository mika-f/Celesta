このページでは、ディスプレイも GPU もない Linux マシンを、ほかのアプリケーションが `celesta` コマンドで動かすレンダリングバックエンドとして用意します。以下のコマンドはすべて、main ブランチからビルドしたまっさらな Ubuntu 22.04（glibc 2.35、x86-64）のコンテナで実行したもので、出力もその実行結果です。

呼び出す側のアプリケーションが使う実行ファイルは `celesta` 1 つだけです。`celesta init` でプロジェクトを作り、`celesta doctor` でマシンと予定のレンダリングを確認し、`celesta plugin` でコーデックを入れ、`celesta export` と `celesta info` で書き出しと情報の取得をします。どれも呼び出し元向けに JSON の結果を 1 つ出力できます。

## 1. システムパッケージをインストール

Celesta は Vulkan で描画します。GPU がなければ、Mesa のソフトウェア Vulkan ドライバ（lavapipe）が CPU で描画します。テキストを描くにはフォントが少なくとも 1 つ必要です。

```sh title="システムパッケージ（Ubuntu 22.04）"
sudo apt-get install -y --no-install-recommends \
  build-essential ca-certificates curl git pkg-config xz-utils libclang-dev \
  libvulkan1 mesa-vulkan-drivers fontconfig fonts-dejavu-core
```

日本語のテキストには `fonts-noto-cjk` も入れてください。

## 2. `celesta` をビルド

`celesta` コマンドの Linux 向けパッケージはまだないので、ソースからビルドします。CI と同じく Rust 1.99.1、Node.js 26、ルートの `package.json` にある pnpm（12.17.0）を使います。

```sh title="celesta をビルド"
git clone https://github.com/mika-f/Celesta.git
cd Celesta
pnpm install --frozen-lockfile
pnpm run build:runtime
cargo build --release --locked -p celesta-cli
mkdir -p ~/.local/bin
ln -s "$PWD/target/release/celesta" ~/.local/bin/celesta
```

ソースからビルドした `celesta` は、React ランタイムをこのチェックアウト（`packages/cli/dist/cli.js`）から読み込み、`PATH` の `node` で実行します。チェックアウトは移動せず、実行ファイルはほかの場所にコピーせずリンクしてください。チェックアウトを移動すると、`celesta doctor` が `checks.node` に `runtime_unavailable` を報告します。

Wayland のセッションがないマシンでは、グラフィックスアダプターを開く `celesta` の実行のたびに、標準エラー出力に `error: XDG_RUNTIME_DIR not set in the environment.` が表示されます。描画には影響しません。表示を止めるには、ワーカーの環境で `XDG_RUNTIME_DIR` を（たとえば `/tmp` に）設定してください。

## 3. プロジェクトを作る

```sh title="プロジェクトを作る"
celesta templates
celesta init film --template blank
cd film
```

```text
Initialized Celesta project in /home/render/qs/film
Edit film.tsx.
Run pnpm install for TypeScript or external dependencies.
```

組み込みのテンプレートには `pnpm install` は不要です。コンポジションが外部パッケージを import するときだけ実行してください。生成される `package.json` のスクリプトは `celesta-editor` と `celesta-exporter` を呼びますが、ヘッドレスのワーカーでは使いません。代わりに `celesta export` を使います。

## 4. マシンを確認

```sh title="Node.js、React ランタイム、グラフィックスを確認"
celesta doctor --json
```

```json
{"schemaVersion":1,"status":"ok","checks":{"node":{"status":"ok","message":"Node.js v26.13.0; /home/render/celesta/crates/react-bridge/../../packages/cli/dist/cli.js"},"graphics":{"status":"ok","message":"llvmpipe (LLVM 15.0.7, 256 bits) (Vulkan)"},"output":{"status":"skipped","message":"no render requested"}}}
```

`llvmpipe` はソフトウェア描画の lavapipe です。`checks.graphics` が失敗するときは [GPU がない Linux](/ja/docs/export/#gpu-がない-linux) を参照してください。

## 5. コーデックプラグインを入れる

静止画にはプラグインは要りません。MOV（アルファ付き ProRes 4444）と WebM（VP9）には標準の `ffmpeg` メディアプラグインが、MP4（H.264）にはさらに任意の `ffmpeg-x264` プラグインが必要です。マシン上のすべてのプロジェクトで使えるように入れます。

```sh title="標準のメディアプラグインを入れる"
celesta plugin install --global ffmpeg --json
```

```json
{"command":"install","plugins":[{"id":"ffmpeg","scope":"global","status":"installed","version":"0.10.0"}],"schemaVersion":1,"scope":"global","status":"ok"}
```

```sh title="MP4 用に H.264 を入れる"
celesta plugin install --global ffmpeg-x264 --json
```

> [!NOTE]
> **Celesta 0.10.0 と Ubuntu 22.04 での MP4**
> レジストリの `linux-x86_64` 向け `ffmpeg-x264` 0.10.0 は glibc 2.38（Ubuntu 24.04 以降）が必要です。Ubuntu 22.04 ではインストールでき、`celesta plugin audit` も問題なしと報告しますが、起動できず、MP4 の書き出しは `component_unavailable` と ``GLIBC_2.38' not found`` で失敗します。0.10.0 より後のリリースは glibc 2.35 向けにビルドされます。それまでは、下のようにチェックアウトから FFmpeg プラグインを 2 つともビルドしてください。

リポジトリのルートで実行します。FFmpeg を 2 回ビルドし、両方のプラグインを `~/celesta-plugins` に置きます。

```sh title="FFmpeg プラグインをソースからビルド（Ubuntu 22.04）"
sudo apt-get install -y --no-install-recommends nasm zlib1g-dev libx264-dev libvpx-dev libopus-dev
FFMPEG_VARIANT=standard scripts/build-ffmpeg-linux.sh ~/ffmpeg8-standard
FFMPEG_VARIANT=x264 scripts/build-ffmpeg-linux.sh ~/ffmpeg8-x264
PKG_CONFIG_PATH=~/ffmpeg8-standard/lib/pkgconfig FFMPEG_X264_PKG_CONFIG_PATH=~/ffmpeg8-x264/lib/pkgconfig \
  node scripts/plugins/build.mts plugins/ffmpeg plugins/ffmpeg-x264 --out ~/celesta-plugins
export CELESTA_PLUGIN_PATH=~/celesta-plugins
```

Celesta はレジストリのプラグインより先に `CELESTA_PLUGIN_PATH` を探すので、レジストリの `ffmpeg-x264` が入っていてもこちらが使われます。1 回の `celesta export` や `celesta doctor` に `--plugin-dir ~/celesta-plugins` を付けても同じです。

`celesta plugin audit` は、入っているプラグインをレジストリのセキュリティ勧告や取り下げられたバージョンと照らし合わせるだけで、起動はしません。プラグインがこのマシンで実際に動くかは、次の手順の `celesta doctor` で予定のレンダリングを試して確かめます。

## 6. 予定のレンダリングを試す

`celesta doctor` に、エントリと書き出す予定の出力を渡します。書き出しと同じメディアコンポーネントで描画して結果を捨て、途中で何か失敗すると 0 以外で終了します。

```sh title="出力形式ごとに試す"
celesta doctor --react film.tsx --frame 0 frame.png --json
celesta doctor --react film.tsx film.mov --json
celesta doctor --react film.tsx film.mp4 --json
```

上の実行では、Ubuntu 22.04 にレジストリの `ffmpeg-x264` 0.10.0 を入れた状態で、ジョブを受ける前に MP4 の確認が失敗しました。

```json
{"schemaVersion":1,"status":"error","checks":{"node":{"status":"ok","message":"…"},"graphics":{"status":"ok","message":"llvmpipe (LLVM 15.0.7, 256 bits) (Vulkan)"},"output":{"status":"error","code":"component_unavailable","message":"ffmpeg-x264/libx264 is unavailable: the media plugin is unavailable: …/celesta-codec-ffmpeg-x264 is unavailable: its process exited; it last printed:\n…: /lib/x86_64-linux-gnu/libc.so.6: version `GLIBC_2.38' not found (required by …/libx264.so.164)"}}}
```

ソースからビルドしたプラグインを `CELESTA_PLUGIN_PATH` に置くと、同じ確認は `"output":{"status":"ok","message":"rendered through the selected media path; output discarded"}` を報告します。

## 7. 書き出す

```sh title="PNG、MOV、MP4 を書き出す"
celesta export --react film.tsx --frame 0 frame.png
celesta export --react film.tsx film.mov
celesta export --react film.tsx film.mp4
```

```text
format: Image · PNG
graphics: vulkan (software) · llvmpipe (LLVM 15.0.7, 256 bits)
rendering frame 1/1  — fps  elapsed 00:00:00  eta —
warning: font family "sans-serif" (weight 400) is not installed or loaded; text layer "root.0.1" uses a fallback font
export complete: frame.png
```

blank テンプレートのテキストは総称ファミリー `sans-serif` を使っており、フォントが入っていても書き出しは `font family "sans-serif" (weight 400) is not installed or loaded` と警告します。テキストは入っている代替フォント（ここでは DejaVu Sans）で描かれます。書体を自分で選ぶには、入っているファミリー名を指定するか、`<Font>` でフォントファイルを読み込んでください。

8 CPU コアの lavapipe では、5 秒・1080p の blank テンプレートを MOV では約 18 fps、MP4 では約 30 fps で描画しました。既存の出力を置き換えるには `--overwrite` を、一部だけ書き出すには `--from` と `--to` を付けます。

## 8. ほかのアプリケーションから呼ぶ

呼び出す側のアプリケーションには、次の決まりを伝えてください。

- `celesta export`、`celesta info`、`celesta doctor`、`celesta plugin list|install|audit` に `--json` を付けます。どれも標準出力に JSON オブジェクトをちょうど 1 つ出力します。プラグインのプロセス自身のエラー出力などの診断は標準エラー出力に出るので、JSON として読むのは標準出力だけにしてください。
- まず終了ステータスを確認します。0 は成功、1 は書き出しや確認の失敗、2 は引数の誤りです。次に `status`（`"ok"` か `"error"`）を読み、失敗なら `error.code` と、あれば `error.hint` を読みます。
- 長いレンダリングでは、`--json` に加えて `--json-progress` を付けると、最後の結果の前に進捗イベントが 1 行ずつ JSON で出力されます。

MP4 の書き出しに成功すると、書いたファイル、それを書いたコンポーネント、グラフィックスアダプターが返ります。

```sh title="呼び出し元向けに書き出す"
celesta export --react film.tsx film.mp4 --overwrite --json
```

```json
{"status":"ok","source":"/home/render/qs/film/film.tsx","composition":{"width":1920,"height":1080,"fps":30.0,"frames":150,"duration":5.0},"graphics":{"backend":"vulkan","adapter":"llvmpipe (LLVM 15.0.7, 256 bits)","software":true},"compositionId":"default","compositions":[{"id":"default","width":1920,"height":1080,"frameRate":{"numerator":30,"denominator":1},"durationInFrames":150}],"outputs":[{"type":"video","path":"/home/render/qs/film/film.mp4","firstFrame":0,"frames":150,"start":0.0,"duration":5.0,"muxer":"ffmpeg/mp4","video":{"component":"ffmpeg-x264/libx264","codec":"avc1.640028"},"audio":null,"alpha":false,"files":["/home/render/qs/film/film.mp4"]}],"warnings":["font family \"sans-serif\" (weight 400) is not installed or loaded; text layer \"root.0.1\" uses a fallback font"],"quality":[],"stats":{"elapsedSeconds":5.61,"renderFps":29.8}}
```

失敗しても形は同じで、呼び出し元が分岐に使えるエラーコードと、運用者向けのヒントが入ります。

```json
{"status":"error","error":{"code":"output_exists","message":"output already exists: frame.png","hint":"pass --overwrite to replace it, or write to another path"},…}
{"status":"error","error":{"code":"unsupported_output","message":"no muxer writes .png files","hint":"PNG, JPEG and WebP outputs are still images: select frames with --frame, as in --frame 0 (or --frame 0,30,60, or --every 30)"},…}
{"status":"error","error":{"code":"project_json","message":"could not load export project: invalid project JSON: expected value at line 1 column 1","hint":"this looks like a React entry: add --react to read it as one"},…}
```

`celesta info --react film.tsx --json` は描画せずにサイズ、フレームレート、コンポジションを返します。`--props` や `--props-file` でジョブのデータをコンポジションに渡せます。フィールドとエラーコードの一覧は[書き出しガイド](/ja/docs/export/#コマンドラインから)に、プラグインのコマンドの JSON は[レジストリのプラグイン](/ja/docs/plugins/#レジストリのプラグイン)にあります。

## ビルドせずに使う: Docker イメージ

`packaging/linux/Dockerfile` は、`celesta`、標準の `ffmpeg` プラグイン、lavapipe、フォントの入ったイメージを作るので、ほかに準備しなくても PNG、MOV、WebM を書き出せます。H.264 のプラグインは入っていないので、MP4 には `ffmpeg-x264` をマウントして `--plugin-dir` で指定します。既定のエントリポイントは `celesta-exporter` なので、`celesta` のサブコマンドは `--entrypoint /opt/celesta/celesta` を付けて実行します。

```sh title="イメージで MOV を書き出す"
docker build -f packaging/linux/Dockerfile -t celesta-exporter .
docker run --rm -u "$(id -u):$(id -g)" -v "$PWD/film:/work" -w /work \
  --entrypoint /opt/celesta/celesta celesta-exporter \
  export --react film.tsx film.mov --json
```

イメージは Ubuntu 26.04 をもとにしているので、そのプラグインはホストの glibc に依存しません。ほかの例は [Linux コンテナの README](https://github.com/mika-f/Celesta/blob/main/packaging/linux/README.md#unified-cli) にあります。
