ソースからビルドするには、**Rust 1.89 以降**、OS に対応したビルドツール、**FFmpeg 8.1.x の開発用ライブラリ**が必要です。React ランタイムのビルドには、CI と同じ Node.js 26 と、ルートの package.json に指定された pnpm を使ってください。

> [!NOTE]
> **開発ライブラリが必要です**
> FFmpeg のコマンドライン実行ファイルだけでは足りません。Celesta のコーデックプラグイン `celesta-codec-ffmpeg` が開発ライブラリにリンクします（アプリと書き出しツールは FFmpeg をリンクしません）。FFmpeg 9 以降にはまだ対応していません。

## 1. OS の準備

::::tabs[ソースをビルドする OS]{default="macOS"}
:::tab[macOS]

Xcode Command Line Tools と Homebrew をインストールします。続いてライブラリを入れ、Celesta をビルドするシェルで `pkg-config` に keg-only の FFmpeg パッケージを指定します。

```sh title="macOS の依存関係"
xcode-select --install
brew install ffmpeg@8 pkg-config
export PKG_CONFIG_PATH="$(brew --prefix ffmpeg@8)/lib/pkgconfig"
```

:::
:::tab[Windows]

MSVC の C++ ビルドツールと vcpkg をインストールします。`VCPKG_ROOT` に vcpkg のチェックアウト先を設定し、以下の FFmpeg パッケージをインストールします。MSVC ツールチェーンを設定したシェルで Cargo を実行してください。

```sh title="Windows の依存関係"
vcpkg install ffmpeg[x264,vpx,opus]:x64-windows-static-md
```

vcpkg は、FFmpeg 8.1.x を提供するリリース（例：2026.07.29）を使ってください。新しいポートでは、未対応の FFmpeg 9 が提供される場合があります。

:::
:::tab[Linux]

Ubuntu 24.04 LTS を含む多くのディストリビューションでは FFmpeg 8.1.x が提供されないため、手順 2 でソースを取得した後、リポジトリの `scripts/build-ffmpeg-linux.sh` でビルドします。`libx264`、`libvpx`、`libopus` を含む静的ライブラリを `/opt/ffmpeg8` に構築し、4 コアで約 5 分かかります。Cargo は `libclang-dev` で FFmpeg のバインディングを生成します。Debian と Ubuntu では、リポジトリのルートで以下を実行します。

```sh title="Linux の依存関係"
sudo apt-get install -y build-essential nasm pkg-config curl xz-utils zlib1g-dev libx264-dev libvpx-dev libopus-dev libclang-dev
sudo apt-get install -y libasound2-dev libxkbcommon-x11-dev libfontconfig-dev
sudo scripts/build-ffmpeg-linux.sh /opt/ffmpeg8
export PKG_CONFIG_PATH=/opt/ffmpeg8/lib/pkgconfig
```

Cargo を実行する各シェルで `PKG_CONFIG_PATH` を設定します。FFmpeg はコーデックプラグインのプログラムにリンクされるので `LD_LIBRARY_PATH` は不要です。ディストリビューションが FFmpeg 8.1.x を提供する場合は、スクリプトの代わりに `pkg-config` と `libavcodec`、`libavformat`、`libavfilter`、`libavdevice`、`libavutil`、`libswscale`、`libswresample` の開発パッケージをインストールしてください。GPU なしでの書き出しは、[動画の書き出し](/ja/docs/export/)をご覧ください。

:::
::::

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

コーデックプラグインを一度ビルドします（ソースを更新したらもう一度）。Celesta は動画と音声をこれで読み書きし、`target/release` の自分の実行ファイルと同じフォルダーから見つけます。

```sh title="コーデックプラグインをビルド"
cargo build -p celesta-codec-ffmpeg --release
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
