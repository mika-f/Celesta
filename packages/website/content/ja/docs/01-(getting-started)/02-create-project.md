React コンポジション、メディア、npm の依存関係を置くフォルダを初期化します。Celesta には、ひな形のプレビューと書き出しに必要なランタイムが含まれます。

## アプリから

**File → Create New Project…**、ツールバーの **Create New Project…**、または macOS の `⌘ N` / Windows・Linux の `Ctrl N` を使います。プロジェクトのフォルダを選びます。選択画面で新規作成もできます。Celesta がそのフォルダを初期化し、`film.tsx` を開きます。

## コマンドラインから

PATH にある `celesta-editor` に、`--init` とフォルダ名を渡します。フォルダを省略すると現在のディレクトリを初期化します。

```sh title="プロジェクトを初期化して開く"
celesta-editor --init my-video
cd my-video
celesta-editor film.tsx
celesta-exporter --react film.tsx output.mp4
```

```sh title="現在のディレクトリを初期化"
celesta-editor --init
```

デスクトップ版の実行ファイル名は `Celesta` と `Celesta-export` です（Windows では `.exe` 付き）。`celesta-editor` と `celesta-exporter` の代わりにインストール先のパスを使ってください。必要に応じて生成されたスクリプト内も変更します。

::::tabs[プロジェクトを初期化する OS]{default="macOS"}
:::tab[macOS]

このコマンドは `/Applications` にアプリをインストールした場合の例です。必要に応じてパスを変更してください。

```sh title="インストールしたアプリで初期化"
"/Applications/Celesta.app/Contents/MacOS/Celesta" --init my-video
```

:::
:::tab[Windows]

PowerShell を使い、`C:\path\to\Celesta` をアプリのインストール先または展開先に置き換えてください。インストーラーは Celesta を PATH に追加しません。

```sh title="インストールしたアプリで初期化"
& "C:\path\to\Celesta\Celesta.exe" --init my-video
```

:::
::::

[ソースビルド](/ja/docs/build-from-source/)の場合は、先に React ランタイムをビルドし、リポジトリのルートから `cargo run -p celesta-editor --release -- --init my-video` を実行します。

## 初期化で生成されるもの

- `film.tsx`：1920 × 1080、30 fps、5 秒のコンポジション。背景とタイトルは `<Group>` の中にあり、エディタの Add メニューはそこへ新しいレイヤーを書き込みます。
- `package.json`：プレビュー・書き出し・型検査のスクリプトと、開発依存として TypeScript `^7.0.2` を持つ非公開プロジェクト。
- `tsconfig.json` と `.celesta/`：Celesta の同梱ランタイムに対応する設定と型宣言。
- `.gitignore`：`node_modules/` と `output.mp4` を除外します。生成される `.celesta/` ディレクトリは自身を除外します。

プレビューと書き出しはパッケージをインストールせずに動作します。プロジェクトの TypeScript コンパイラを使うには、Node.js と pnpm を別途インストールし、プロジェクトのフォルダで以下を実行します。

```sh title="TypeScript コンパイラをインストールして型検査"
pnpm install
pnpm typecheck
```

> [!NOTE]
> **既存のフォルダの初期化**
> 再度初期化すると `.celesta/` が更新され、不足するひな形ファイルが作られます。既存のソース、`package.json` の依存関係とスクリプト、`tsconfig.json`、`.gitignore` は保持されます。既存の TypeScript 設定が `./.celesta/tsconfig.json` を継承していない場合、Celesta が必要な変更を表示します。自分の設定に取り込んでください。

## npm の依存関係を追加

外部パッケージは、インポートするプロジェクトにインストールします。Celesta リポジトリの外にあるプロジェクトでは、そのフォルダで実行します。

```sh title="プロジェクトの依存関係を追加"
pnpm add ag-psd
```

プレビューと書き出しでは、読み込み元のプロジェクトの `node_modules` から外部パッケージを解決します。すべての `@celesta/*` パッケージと React はランタイムに含まれているため、外部プロジェクトに追加する必要はありません。

Node の素材準備スクリプトも同じ依存関係を使います。`package.json` の隣に `prepare-assets.mjs` として保存し、同じフォルダに `portrait.psd` を置きます。

```tsx title="prepare-assets.mjs"
import { readFileSync, writeFileSync } from 'node:fs';
import { readPsd } from 'ag-psd';

const { width, height } = readPsd(
  readFileSync(new URL('./portrait.psd', import.meta.url)),
  { skipLayerImageData: true, skipCompositeImageData: true, skipThumbnail: true },
);
writeFileSync(
  new URL('./portrait-info.json', import.meta.url),
  JSON.stringify({ width, height }),
);
```

```sh title="素材を準備"
node prepare-assets.mjs
```

生成された JSON は通常どおりコンポジションでインポートします。直接実行する Node スクリプトは自分でインストールした Node.js を、コンポジションの評価はアプリ同梱の Node.js を使います。`.ts` スクリプトには、TypeScript の型除去に対応した Node.js、またはプロジェクトにインストールした TypeScript ランナーを使ってください。

## リポジトリ内のサンプルの依存関係

`examples/` 内の React サンプルは pnpm ワークスペースに属します。リポジトリのルートで `pnpm install` を実行し、必要なサンプルに依存関係を追加します。

```sh title="サンプルに依存関係を追加してプレビュー"
pnpm --dir examples/versus add ag-psd
cargo run -p celesta-editor --release -- examples/versus/film.tsx
cargo run -p celesta-exporter --release -- --react examples/versus/film.tsx output.mp4
```

サンプルのマニフェストはローカルの Celesta パッケージに `workspace:*` を使い、Node のツールからもインポートを解決できます。Cargo コマンドを実行する前に[ソースビルドの準備](/ja/docs/build-from-source/)を済ませてください。

[最初の React コンポジション](/ja/docs/react-compositions/)、[プレビュー操作](/ja/docs/preview/)、[MP4 の書き出し](/ja/docs/export/)へ進みましょう。
