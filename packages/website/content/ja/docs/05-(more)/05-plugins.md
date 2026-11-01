コンテンツプラグインは、Celesta 本体にない機能を追加します。独自のコードで描くレイヤーや、音声合成チャンネルのようなコンポーネントライブラリです。プラグインはコンピューターに置くフォルダーで、コンポジションからは `@celesta-plugin/<id>` としてコンポーネントを import します。

プラグインは新しい機能で、今後も変わる可能性があります。デスクトップアプリのプレビューと `celesta-exporter` で動作し、`@celesta/web` と CPU リファレンスレンダラーでは動作しません。

## プラグインを入れる

プラグインは `celesta-plugin.json` のあるフォルダーです。いちばん簡単なのは、ユーザーのプラグインフォルダーに置く方法です。

| OS | ユーザーのプラグインフォルダー |
| --- | --- |
| Windows | `%APPDATA%\Celesta\plugins` |
| macOS | `~/Library/Application Support/Celesta/plugins` |
| Linux など | `$XDG_DATA_HOME/celesta/plugins`（未設定なら `~/.local/share/celesta/plugins`） |

Celesta は次の順にプラグインを探し、同じ id のプラグインが複数あれば先に見つかったものを使います。

1. エクスポーターの各 `--plugin-dir`
2. 環境変数 `CELESTA_PLUGIN_PATH` の各フォルダー（Windows は `;`、それ以外は `:` 区切り）
3. ユーザーのプラグインフォルダー
4. Celesta に同梱のプラグインフォルダー（Windows は実行ファイルの隣の `plugins`、macOS のアプリは `Contents/PlugIns`、それ以外は実行ファイルのフォルダーから見た `../lib/celesta/plugins`）

1 と 2 で指定するフォルダーはプラグインそのものでも、複数のプラグインを入れたフォルダーでもかまいません。`CELESTA_PLUGIN_PATH` には絶対パスを指定してください。相対パスは、Celesta を起動した場所によって指すフォルダーが変わるため読み飛ばされます。

macOS と Linux では、3 と 4 のフォルダーやプラグインを他のユーザーが書き換えられる場合（グループやその他に書き込み権限がある、所有者が root でも自分でもない）は使いません。プラグインのファイルを書き換えられる人は、それを使う人の権限でコードを動かせるからです。`chmod go-w` で書き込み権限を外してください。

```sh
celesta-exporter --react film.tsx out.mp4 --plugin-dir ~/celesta-plugins
```

## 信頼と承認

プロジェクトを開くと、そのコードがコンピューター上で動きます。プラグインも自分のコードを動かします。そのためデスクトップアプリは、どちらも動かす前に確認します。

- **プロジェクト**：初めて開くとき、そのフォルダーを信頼するかを尋ねます。信頼するまでは何も評価しません。インターネットからダウンロードしたプロジェクトは、その旨が確認に表示されます（Windows）。**File > Create New Project** で作ったプロジェクトは最初から信頼済みです。信頼はフォルダーとその中すべてに及び、記憶されます。
- **プラグイン**：ユーザーのプラグインフォルダーにあるプラグインと、プロジェクトが持ち込むプラグインは、許可するまで動きません。見つけたときに確認し、更新などでプラグインのファイルが変わったときにも改めて確認します。許可するまでそのレイヤーは描かれず、Problems に承認待ちと表示されます。`--plugin-dir` や `CELESTA_PLUGIN_PATH` で指定したプラグインと、Celesta に同梱のプラグインは確認しません。

選んだ内容は Celesta の設定フォルダーの `settings.json` に保存されます。設定フォルダーは Windows では `%APPDATA%\Celesta`、macOS では `~/Library/Application Support/Celesta`、それ以外では `$XDG_CONFIG_HOME/celesta`（未設定なら `~/.config/celesta`）です。

`celesta-exporter` と MCP サーバーは、指定されたものをそのまま動かし、確認はしません。

### プロジェクトに同梱するプラグイン

プロジェクトは自分のフォルダーにプラグインを入れて持ち込めます。プロジェクトのルートにある `celesta.config.json` に、そのフォルダーを書きます。

```json
{
  "plugins": {
    "paths": ["./plugins"]
  }
}
```

各パスはプロジェクトのルートからの相対パスで、ルートの中を指す必要があります。デスクトップアプリは、プロジェクトを信頼したあと、これらのフォルダーを `CELESTA_PLUGIN_PATH` の次、ユーザーのプラグインフォルダーの前に探します。`celesta.config.json` のあるフォルダーが、信頼の確認で表示されるプロジェクトのルートにもなります。このファイルがなければ、エントリーのあるフォルダーがルートです。`celesta-exporter` はこのファイルを読まないので、フォルダーを `--plugin-dir` で指定してください。

## プラグインを読み込み直す

デスクトップアプリはプロジェクトを開くたびにプラグインを探します。アプリを開いたまま入れたプラグインは、**File > Reload Plugins** で読み込めます。読み飛ばしたプラグインとその理由は Problems タブに表示されます。レイヤーを描くプラグインは独立したプログラムとして動き、フレームに必要になった時点で Celesta が起動します。プログラムはプラグインに同梱され、対応するプラットフォームごとに用意されます。

## コンポーネントを使う

```tsx
import { Composition } from '@celesta/react';
import { Flag } from '@celesta-plugin/flag';

export default function Root() {
  return (
    <Composition width={1280} height={720} fps={30} durationInFrames={150}>
      <Flag src="./portrait.png" x={80} y={160} amplitude={0.1} />
    </Composition>
  );
}
```

どのフォルダーにもないプラグインを import したエントリーは読み込みに失敗し、プラグイン名がエラーに表示されます。フレームは時刻だけで決まります。プラグインのレイヤーも props とフレームだけで決まるため、プレビュー・書き出し・シークの結果が一致します。

プロジェクトの TypeScript サポートは、入っている各プラグインの型定義を `.celesta/plugins/` にコピーするので、エディターで props を補完できます。

## サンプル

リポジトリの `examples/plugins` に、出発点になるプラグインが 2 つあります。

| プラグイン | 内容 |
| --- | --- |
| `flag` | プラグインのプログラムが描くレイヤー。画像を旗のようになびかせ、Celesta がテクスチャ付きメッシュとして描きます。 |
| `beep-voice` | コンポーネントだけのプラグイン。音声合成エンジンの代わりにゲーム風のビープ音で `planDialogue()` のセリフを読み上げ、口パクも付ける音声チャンネルです。 |

```sh
node examples/plugins/build.mjs
cargo run -p celesta-exporter -- --react examples/plugins/flag/demo.tsx flag.mp4 --plugin-dir examples/plugins/dist
```

## プラグインを作る

プラグインは最大 2 つの部分からなり、どちらも言語を問いません。

- **コンポーネント**（マニフェストの `module` と `types`）：ES モジュールとその型定義です。Celesta はモジュールをエントリーにバンドルし、その中の `react` と `@celesta/*` の import はアプリ自身のものを使います。音声合成チャンネルはこれだけで作れます。`@celesta/voicevox` と同じく `prepare()` の中で動き、`audio` と `lipSync` を付けたセリフを返します。
- **プログラム**（`executable`、プラットフォームごと）：標準入力でリクエストを読み、標準出力で答えます。レイヤーについては、指定された時刻のテクスチャ付き三角形メッシュと、その色・マスク・合成方法を返し、Celesta が描きます。Rust なら `celesta-plugin` クレートを使えます。プロトコル自体は長さ付きの素朴な JSON です。

プラグインの id には英小文字・数字・`-`・`_`・`.` を使い、先頭は英小文字か数字にします。マニフェストで指定するファイルはすべて、プラグインのフォルダー内を指す相対パスにします。`..` を含むパスや、フォルダーの外へ向かうリンクは使えません。これに反するプラグインは、理由を示したうえで読み飛ばされます。

両方の部分の作り方はサンプルの README で、マニフェストとプロトコルの詳細はリポジトリの `docs/handoff/2026-11-01-plugin-system.md` で説明しています。
