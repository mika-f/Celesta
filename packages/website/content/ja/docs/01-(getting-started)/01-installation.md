リリースの添付ファイルから、OS に合う Celesta パッケージをダウンロードします。対応 OS のバージョンとアーキテクチャはリリースノートで確認してください。

- [macOS 用をダウンロード ↓](celesta:download/macos)
- [Windows 用をダウンロード ↓](celesta:download/windows)

ほかのバージョンやアーキテクチャをお探しですか？ [すべてのリリースを見る](celesta:download/all)。

> [!NOTE]
> **Celesta の実行に必要なものを同梱**
> macOS と Windows のパッケージには React ランタイム、Node.js、動画と音声を読み書きする FFmpeg コーデックプラグインが含まれています。動画の作成と書き出しのために Rust、Node.js、pnpm、FFmpeg を別途インストールする必要はありません。

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

Linux では[ソースからのビルド手順](/ja/docs/build-from-source/)を使います。リリースに追加のプラットフォーム向けパッケージがあるか、添付ファイルも確認してください。

:::
::::

## 2. プロジェクトを作成

**File → Create New Project…** またはツールバーの **Create New Project…** を選び、`Documents/my-video` などのフォルダを指定します。フォルダ選択画面で新規フォルダを作れます。Celesta が初期化し、`film.tsx` を開きます。プロジェクトはインストールしたアプリと別の場所に置いてください。CLI の操作と生成ファイルは、[プロジェクトの作成](/ja/docs/create-project/)をご覧ください。

## 3. 最初のシーンを編集

テキストエディタで `film.tsx` を開き、タイトルや色を変えて保存します。Celesta がプレビューを再読み込みします。`Space` で再生できます。アニメーションを試すには、[React のタイトルサンプル](/ja/docs/react-compositions/)を `first-scene.tsx` として保存し、**File → Open…** から選択します。

アプリには `title.tsx` などの React サンプルが含まれています。保存場所は[サンプルの案内](/ja/docs/examples/)を参照してください。

## Celesta の更新

Celesta を終了し、新しいリリースのパッケージをインストールします。Windows のポータブル版は、新しいバージョンを別のフォルダに展開して起動します。プロジェクトとメディアを Documents に置けば、アプリの更新から独立して保管できます。

Celesta 本体の開発には、[ソースからのビルド](/ja/docs/build-from-source/)をご覧ください。
