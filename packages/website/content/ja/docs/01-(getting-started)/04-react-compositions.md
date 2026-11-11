インストールしたアプリには React ランタイムが含まれています。アプリまたは CLI で[プロジェクトを作成](/ja/docs/create-project/)すると、ひな形の `film.tsx` と TypeScript 設定を取得できます。ひな形はパッケージのインストールやビルドなしでプレビュー・書き出しできます。追加の npm 依存関係はプロジェクトのフォルダに入れてください。

## 最初のシーンを作る

自分のプロジェクトに `first-scene.tsx` を作ります。このサンプルは 1920 × 1080、30 fps のコンポジションです。タイトルが 30 フレームかけてフェードインし、そのまま 5 秒の終了まで表示されます。

```tsx file=examples/first-scene.tsx title="first-scene.tsx"
```

Celesta の **File → Open…** で `first-scene.tsx` を選び、`Space` で再生します。

テキストや色を変えて保存すると、ネイティブプレビューが再読み込みされます。Celesta のコンポーネントは動画のレイヤーを表します。HTML 要素や CSS クラスの代わりに `Text`、`Rect`、`Group` を使います。

## コンポジションの構成

- ファイルの **default export** は、サイズ・フレームレート・長さを持つ1つ以上のルート `<Composition>` を返します。
- 内側は順番に描画され、後のレイヤーが前のレイヤーの上に重なります。
- 自作コンポーネントは通常の React 関数コンポーネントです。props、`.map()`、条件分岐もいつもどおり使えます。
- 任意の名前付きエクスポート `prepare()` は、最初のフレームの前に一度だけ実行されます。[データ・プロパティ・コンポーネント](/ja/docs/data/)をご覧ください。

## 複数のコンポジションを登録する

各ルートに空でない一意の `id` を付け、フラグメントで返します。サイズ・fps・尺はそれぞれ設定できます。既存の単一コンポジションは `id` を省略したまま使え、その ID は `default` です。選択しない場合は宣言順の先頭を使い、存在しない ID を指定した場合はエラーになります。

```tsx
import { Composition, Text } from '@celesta.studio/react';

export default () => <>
  <Composition id="landscape" width={1920} height={1080} fps={30} durationInFrames={150}>
    <Text>Landscape</Text>
  </Composition>
  <Composition id="portrait" width={1080} height={1920} fps={30} durationInFrames={150}>
    <Text>Portrait</Text>
  </Composition>
</>;
```

アプリのタイトルバーにある **Composition: landscape** で、プレビューと 書き出しページの書き出し対象を切り替えます。`Celesta film.tsx --composition portrait` で特定の ID を開くこともできます。

```sh
Celesta-export --react film.tsx --composition portrait output.mp4 --json
node skills/celesta/scripts/inspect.mjs film.tsx --composition portrait --json
```

両方の JSON 出力に `compositionId` と `compositions` が含まれます（inspect では `ready` の中）。一覧には各ルートの ID・サイズ・フレームレート・フレーム数が入ります。`defineProjectProperties`、`props.json`、`--props`、`prepare()` はエントリ全体で共有し、選択されたコンポジションの子だけをマウントします。

## エディタの TypeScript 設定

React コンポジションを開き、**File → Set Up TypeScript** を選びます。Celesta がすべての `@celesta.studio/*` パッケージ、React、Node.js の対応する型宣言をローカルの `.celesta/` ディレクトリにコピーします。

`.celesta/` と `tsconfig.json` は、プロジェクトのルートに置きます。ルートは、コンポジションのあるフォルダーから上にたどって最も近い、`celesta.config.json` があるフォルダーです。なければコンポジションのあるフォルダーです。上のフォルダーにある `package.json` や `tsconfig.json` は見ないので、プロジェクトの外に書き込むことはありません。`--init` や **Create New Project** で作ったプロジェクトには、コンポジションの隣に `celesta.config.json` がすでにあります。

まだコンポジションがない新規プロジェクトや、コンポジションを `src/` などのサブフォルダーに置くプロジェクトでは、**File → Set Up TypeScript in Folder…** でプロジェクトのフォルダーを選びます。Celesta はそこに型を用意し、`celesta.config.json` がなければ空のものを置きます。これで、サブフォルダーにあるコンポジションもそのフォルダーをルートにします。このような形のプロジェクトで、以前のバージョンでプロジェクトのフォルダーに TypeScript を設定していた場合は、一度これを実行すると、その `.celesta/` がまた更新されるようになります。

`tsconfig.json` がなければ Celesta が作成します。既存の設定がある場合は、ほかの設定を残して以下の `extends` を追加します。

```json title="tsconfig.json"
{
  "extends": "./.celesta/tsconfig.json"
}
```

この設定に必要な型宣言を npm から入れる必要はありません。新しいアプリでプロジェクトを開くと `.celesta/` が更新されます。このディレクトリは Git から自身を除外します。

## `@celesta/*` を読み込むプロジェクト

Celesta のパッケージ名は `@celesta/*` から `@celesta.studio/*`（`@celesta.studio/react` など）に変わりました。旧名を読み込むコンポジションもそのまま動きます。アプリ、CLI、ウェブエディタは `@celesta/react` を `@celesta.studio/react` として解決し、`.celesta/tsconfig.json` は両方の名前を同じ型宣言に対応づけます。旧名を読み込むファイルごとに `scenes/Intro.tsx imports @celesta/react, an old name; import @celesta.studio/react instead` のような警告が出ます。表示先は、アプリの **Problems** タブ、`Celesta-export` の出力（`--json` レポートの `warnings` を含む）、ウェブエディタのブラウザコンソールです。新しいコード、テンプレート、アプリによる編集は `@celesta.studio/*` を使います。

プロジェクト内のインポートをまとめて書き換えるには、Celesta スキルの移行スクリプトを実行します。書き換えたファイルと同じ場所にある `package.json` の `@celesta/*` 依存関係も改名します。

```sh
node skills/celesta/scripts/migrate-packages.mjs path/to/project --dry-run
node skills/celesta/scripts/migrate-packages.mjs path/to/project
```
