リリースの添付ファイルから、OS に合う Celesta パッケージをダウンロードします。対応 OS のバージョンとアーキテクチャはリリースノートで確認してください。

- [macOS 用をダウンロード ↓](celesta:download/macos)
- [Windows 用をダウンロード ↓](celesta:download/windows)

ほかのバージョンやアーキテクチャをお探しですか？ [すべてのリリースを見る](celesta:download/all)。

> [!NOTE]
> **Celesta の実行に必要なものを同梱**
> macOS と Windows のパッケージには React ランタイム、Node.js、動画と音声を読み書きする FFmpeg メディアプラグインが含まれています。動画の作成と書き出しのために Rust、Node.js、pnpm、FFmpeg を別途インストールする必要はありません。

FFmpeg の構成のうち、同梱するのは **標準の `ffmpeg` だけ**です。メディアの読み込みと
ProRes／MOV、WebM（VP9 と Opus）の書き出しに使えます。macOS は AVFoundation、Windows は
Media Foundation も同梱し、利用可能な H.264 エンコーダーで MP4 を書き出せます。
Linux の H.264／MP4 には追加の **ffmpeg-x264** が必要です。
[追加コーデックのダウンロード](/ja/docs/plugins/#追加コーデックのダウンロード)をご覧ください。

## 1. アプリをインストール

::::tabs[インストール先の OS]{default="macOS"}
:::tab[macOS]

1. Apple Silicon では `macos-arm64.dmg`、Intel Mac では `macos-x64.dmg` を選びます。完全なファイル名にはリリースのバージョンが含まれます。
2. ディスクイメージを開き、**Celesta** を **Applications** にドラッグします。
3. ディスクイメージを取り出し、Applications から Celesta を開きます。

:::
:::tab[Windows]

`windows-x64-setup.exe` インストーラーをダウンロードして実行し、スタートメニューから Celesta を開きます。完全なファイル名にはリリースのバージョンが含まれます。

ポータブル版を使う場合は `windows-x64.zip` をダウンロードし、アーカイブ全体を展開して `Celesta.exe` を起動します。ランタイムのディレクトリと DLL は実行ファイルの隣に置いてください。

:::
:::tab[Linux]

x86_64 のヘッドレス Linux（Ubuntu 22.04 以降、glibc 2.35 以上）では、リリースの添付ファイルから
`Celesta-<version>-linux-x64.tar.gz` をダウンロードします。統合 CLI、単体のエクスポーター、
Node.js、React ランタイム、標準 FFmpeg プラグインが同梱されます。デスクトップエディターは含まれません。

Ubuntu では、システムライブラリ、ソフトウェア Vulkan ドライバー、フォントをインストールします
（Vulkan 対応のハードウェアドライバーがある場合はそれも使用できます）。

```sh
sudo apt-get install libatomic1 zlib1g libvulkan1 mesa-vulkan-drivers fonts-dejavu-core
tar -xzf Celesta-<version>-linux-x64.tar.gz
./Celesta-<version>-linux-x64/bin/celesta doctor
```

`<version>` はリリースのバージョンに置き換えてください。移動する場合は、展開したディレクトリ全体を
移動してください。`celesta` を PATH から実行するには、その中の `bin` ディレクトリを追加します。
CLI で始めるには[プロジェクトの作成](/ja/docs/create-project/)、Linux でデスクトップエディターを使うには
[ソースからビルド](/ja/docs/build-from-source/)をご覧ください。

:::
::::

## 2. プロジェクトを作成

**File → Create New Project…** またはツールバーの **Create New Project…** を選び、`Documents/my-video` などのフォルダを指定します。フォルダ選択画面で新規フォルダを作れます。Celesta が初期化し、`film.tsx` を開きます。プロジェクトはインストールしたアプリと別の場所に置いてください。CLI の操作と生成ファイルは、[プロジェクトの作成](/ja/docs/create-project/)をご覧ください。

## 3. 最初のシーンを編集

テキストエディタで `film.tsx` を開き、タイトルや色を変えて保存します。Celesta がプレビューを再読み込みします。`Space` で再生できます。アニメーションを試すには、[React のタイトルサンプル](/ja/docs/react-compositions/)を `first-scene.tsx` として保存し、**File → Open…** から選択します。

アプリには `title.tsx` などの React サンプルが含まれています。保存場所は[サンプルの案内](/ja/docs/examples/)を参照してください。

## Celesta の更新

いま使っているバージョンは、**Celesta → About Celesta** で確認できます。このウィンドウには OS、GPU、Node.js のバージョンも表示されます。バージョンをクリックするとこれらがまとめてコピーされるので、不具合の報告などに貼り付けられます。

Celesta を終了し、新しいリリースのパッケージをインストールします。Windows のポータブル版は、新しいバージョンを別のフォルダに展開して起動します。プロジェクトとメディアを Documents に置けば、アプリの更新から独立して保管できます。

Celesta 本体の開発には、[ソースからのビルド](/ja/docs/build-from-source/)をご覧ください。
