ソースからビルドするには、**Rust 1.89 以降**、OS に対応したビルドツール、**FFmpeg 8.1.x の開発用ライブラリ**が必要です。React ランタイムのビルドには、CI と同じ Node.js 26 と、ルートの package.json に指定された pnpm を使ってください。

> [!NOTE]
> **開発ライブラリが必要です**
> FFmpeg のコマンドライン実行ファイルだけでは足りません。Celesta のメディアプラグイン（リポジトリの `plugins/ffmpeg`）が開発ライブラリにリンクします（アプリと書き出しツールは FFmpeg をリンクしません）。FFmpeg 9 以降にはまだ対応していません。

## 1. OS の準備

メディアプラグインは FFmpeg 8.1.x を 2 つのビルドに分けます。`ffmpeg` は標準の
LGPL ビルド（デコード、ProRes、VP9、AAC、Opus、PCM、muxer）で、zlib と、BSD
ライセンスの libopus・libvpx をリンクします。追加の `ffmpeg-x264` は libx264 による
H.264 を提供し、GPL を有効にするのはこのビルドだけです。2 つのビルド先の prefix は
必ず別々にしてください。

手順 2 でソースを取得した後、リポジトリのルートで OS に合うコマンドを実行します。
プラグインをビルドするシェルで、設定した環境変数を保持してください。

::::tabs[Source build platform]{default="macOS"}
:::tab[macOS]

Xcode Command Line Tools と Homebrew を用意します。スクリプトは、それぞれ別の
prefix に FFmpeg の dylib を構築します。

```sh title="ネイティブビルドの依存関係"
brew install pkg-config nasm x264 libvpx opus
export PKG_CONFIG_PATH="$(brew --prefix x264)/lib/pkgconfig:$(brew --prefix opus)/lib/pkgconfig:$(brew --prefix libvpx)/lib/pkgconfig"
for variant in standard x264; do
  FFMPEG_VARIANT="$variant" scripts/build-ffmpeg-linux.sh "$PWD/plugins/target/ffmpeg/$variant"
done
export PKG_CONFIG_PATH="$PWD/plugins/target/ffmpeg/standard/lib/pkgconfig"
export FFMPEG_X264_PKG_CONFIG_PATH="$PWD/plugins/target/ffmpeg/x264/lib/pkgconfig"
```

標準版は Homebrew の `opus` と `libvpx`、x264 版は `x264` をリンクします。スクリプトの
実行中は、それらの `pkgconfig` フォルダーを `PKG_CONFIG_PATH` に含めてください。

ローカル利用には Homebrew の `ffmpeg@8` も標準プラグインで使えます。
`brew install ffmpeg@8 pkg-config` の後、
`PKG_CONFIG_PATH="$(brew --prefix ffmpeg@8)/lib/pkgconfig"` を設定します。
GPL 構成も利用できますが、標準プラグインは libx264 のエンコーダーを公開しません。

:::
:::tab[Windows]

Windows では vcpkg 2026.07.29（FFmpeg 8.1.x）と MSVC のビルドツールを使います。
`ffmpeg[zlib,opus,vpx]`（標準）と `ffmpeg[x264]` を `x64-windows-static-md` triplet で、
それぞれ別の vcpkg インストール先に構築します。`VCPKG_ROOT` に標準版のルート、
`FFMPEG_X264_VCPKG_ROOT` に x264 版のルートを指定します（triplet は必要に応じて
`FFMPEG_X264_VCPKGRS_TRIPLET` で指定できます）。完全な設定例はリポジトリの
setup-ffmpeg CI アクションにあります。ローカル利用には開発ライブラリ付きのシステム FFmpeg 8.1.x
（GPL 構成を含む）も使えます。配布パッケージには上記の構成の分離が必要です。

:::
:::tab[Linux]

スクリプトは FFmpeg の静的ライブラリを構築します。Debian と Ubuntu では、
ネイティブビルドとエディターに必要なシステムライブラリを先にインストールします。

```sh title="ネイティブビルドの依存関係"
sudo apt-get install -y build-essential nasm pkg-config curl xz-utils zlib1g-dev libx264-dev libvpx-dev libopus-dev libclang-dev
sudo apt-get install -y libasound2-dev libxkbcommon-x11-dev libfontconfig-dev
for variant in standard x264; do
  FFMPEG_VARIANT="$variant" scripts/build-ffmpeg-linux.sh "$PWD/plugins/target/ffmpeg/$variant"
done
export PKG_CONFIG_PATH="$PWD/plugins/target/ffmpeg/standard/lib/pkgconfig"
export FFMPEG_X264_PKG_CONFIG_PATH="$PWD/plugins/target/ffmpeg/x264/lib/pkgconfig"
```

標準版には `libopus-dev` と `libvpx-dev`、x264 版には `libx264-dev` が必要です。

:::
::::

ローカルビルドではシステムの FFmpeg 8.1.x も利用できます。配布用には
`scripts/plugins/build.mts` に `--distribution` を指定します。エンコーダーを実際に開き、
明示的な `--check-license` コマンドで検査してから配置します。標準版の
配布には LGPL のみの FFmpeg が必要です。配布用の検査は対象 OS・アーキテクチャ上で
実行してください。クロスビルドで検査を省略することはできません。

## 2. ソースを取得

```sh title="Celesta をクローン"
git clone https://github.com/mika-f/Celesta.git celesta
cd celesta
```

## 3. React ランタイムをビルドする

アプリを起動する前に、ワークスペースの依存関係をインストールし、React ランタイムをビルドします。このビルドは Rust のソースから共有の型定義も生成するので、ソースを更新したあとにも実行し直してください。

```sh title="ソースから React ランタイムをビルド"
pnpm install
pnpm run build:runtime
```

## 4. React コンポジションを開く

メディアプラグインを一度ビルドします（ソースを更新したらもう一度）。Celesta は動画と音声をこれらで読み書きし、`CELESTA_PLUGIN_PATH` から見つけます。`plugins/avfoundation`（VideoToolbox と AVFoundation）は macOS でだけ、`plugins/mediafoundation`（Media Foundation）は Windows でだけビルドされます。それ以外の OS ではスクリプトが `avfoundation: skipped, it is built for macos only` または `mediafoundation: skipped, it is built for windows only` と表示して残りをビルドするので、Linux では FFmpeg だけがビルドされます。この変数には絶対パスを指定し、Celesta を実行する各シェルで設定してください。

```sh title="メディアプラグインをビルド"
node scripts/plugins/build.mts plugins/ffmpeg plugins/ffmpeg-x264 plugins/avfoundation plugins/mediafoundation
export CELESTA_PLUGIN_PATH="$PWD/plugins/dist"   # PowerShell: $env:CELESTA_PLUGIN_PATH = "$PWD\plugins\dist"
```

```sh title="ネイティブアプリを起動"
cargo run -p celesta-editor --release -- packages/cli/examples/title.tsx
```

タイトルのサンプルが開き、プレビュー、タイムライン、Assets パネル、Inspector が表示されます。別の React ファイルを指定して開くこともできます。

```sh title="サンプルプロジェクトを開く"
cargo run -p celesta-editor --release -- examples/reel/film.tsx
```

リポジトリのルートで実行します。`examples/` 内の React サンプルは pnpm ワークスペースのパッケージなので、依存関係もインストールされます。自分のプロジェクトをはじめるには、以下を実行します。

```sh title="ソースビルドでプロジェクトを初期化"
cargo run -p celesta-editor --release -- --init my-video
```

生成されるファイルと依存関係の設定は、[プロジェクトの作成](/ja/docs/create-project/)をご覧ください。

```sh title="ソースビルドで React をプレビュー"
cargo run -p celesta-editor --release -- packages/cli/examples/title.tsx
```

```sh title="ソースビルドで React を書き出し"
cargo run -p celesta-exporter --release -- --react packages/cli/examples/title.tsx output.mp4
```

Cargo コマンドはリポジトリのルートで実行します。パッケージ版の利用者はこの節を飛ばして、[最初の React コンポジション](/ja/docs/react-compositions/)に進めます。
