::::accordion[トラブルシューティング]{multiple}
:::toggle[同梱ランタイムが見つからない]{open}

macOS ではアプリ全体を Applications にコピーしてから起動します。Windows のポータブル版は全体を展開し、`runtime` と DLL を `Celesta.exe` の隣に置きます。不足するファイルがある場合は、再展開または再インストールしてください。

:::
:::toggle[ソースビルドで FFmpeg が見つからない]

実行ファイルだけでなく、8.1.x の開発ライブラリが入っているか確認します。macOS ではメディアプラグインをビルドするシェルで `PKG_CONFIG_PATH` を設定します。Windows は `VCPKG_ROOT` と MSVC ビルドツールを確認し、Linux は `PKG_CONFIG_PATH` に `/opt/ffmpeg8-standard/lib/pkgconfig` などのビルド済み FFmpeg を指定します。[ソースビルドの準備](/ja/docs/build-from-source/)をご覧ください。

:::
:::toggle[動画や音声が表示されない、書き出しが unsupported_output や no_decoder で失敗する]

Celesta は動画と音声をメディアプラグインで読み書きします。プラグインがないと、Problems タブに **No media decoder can run** と表示されます。パッケージでは、Celesta に同梱のプラグインフォルダー（`Celesta.exe` の隣の `plugins\ffmpeg`、macOS アプリでは `Contents/Resources/PlugIns/ffmpeg`）に `ffmpeg` プラグインが残っているか確認するか、パッケージを入れ直してください。ソースビルドでは、リポジトリのルートで `node scripts/plugins/build.mts plugins/ffmpeg` を実行し、Celesta を実行するシェルで `CELESTA_PLUGIN_PATH` に `plugins/dist` の絶対パスを設定します（`export CELESTA_PLUGIN_PATH="$PWD/plugins/dist"`）。古いチェックアウトでビルドした `plugins/dist/ffmpeg` は、`celesta-plugin.json` に `"roles": ["codec"]` があることがあり、Celesta はそれを無視します。ビルドし直してください。メディアプラグインがないと、動画の書き出しはその拡張子を書けるマルチプレクサーがないので `unsupported_output` で失敗し、そのヒントがプラグインの入れ方を伝えます。静止画の書き出しは動画や音声のファイルを読むときに `no_decoder` で失敗します。プラグインの FFmpeg のビルドに動画に必要なエンコーダーがないとき、または追加のエンコーダープラグインがない場合（H.264 は同梱のプラットフォームエンコーダーが利用できない場合に `ffmpeg-x264` が必要）、書き出しは `component_unavailable` で理由とともに失敗します。`Celesta-export --list-components --check` で、どのコンポーネントが使えるかを確認できます。入れてあっても許可していない `ffmpeg-x264` は読み飛ばされるので、そのエンコーダーはインストールされていない扱いになります。Plugins タブか `--allow-plugin ffmpeg-x264` で許可してください。`cargo install` で入れた `celesta-codec-ffmpeg-x264` が `--manifest` の検査に失敗したときは、代わりに `skipped at discovery (…)` とプログラム自身の理由（libx264 のない FFmpeg でビルドされている、など）が表示されます。プラットフォームの H.264 エンコーダーが見つかっても利用できない場合は、追加プラグインを入れて `ffmpeg-x264/libx264` を選ぶか、`.mov` にして標準プラグインの ProRes を使ってください。コンポーネントがインストールされていないときは、コマンドラインでも書き出しページでも、エラーのヒントが、入っているコンポーネントの確かめ方と、そのプラグインが許可を待っているときの許可のしかたを伝えます。[メディアプラグイン](/ja/docs/plugins/#メディアプラグイン)をご覧ください。

書き出しが `unsupported_media` で失敗したときは、デコーダーは見つかったものの、どれもそのファイルを読めませんでした。MP4 や WAV のような `ffmpeg` プラグインが読める形式に変換するか、そのファイルを読めるデコーダーを持つプラグインを入れてください。

:::
:::toggle[メディアが見つからない]

ローカルファイルのパスはコンポジションのファイルを基準に確認し、プロジェクトを共有するときは素材も一緒に渡してください。HTTP(S) の素材は初回に通信が必要です。ネイティブの描画では URL ごとにキャッシュし、更新確認は行わないため、素材を更新したら URL を変えるかキャッシュを削除します。ブラウザでは CORS によるアクセス許可も必要です。

:::
:::toggle[保存してもプレビューが変わらない]

React コンポジションは自動で再読み込みされます。**File → Reload** で再読み込みし、Celesta で開いているファイルを保存したか確認してください。アプリ内ではソースファイルを編集しません。

:::
:::toggle[フォントが違う]

`fontFamily` は描画するコンピューターのインストール済みフォント、または `<Font>` で読み込んだファイルのファミリー名と一致する必要があります。一致しないと別のフォントに切り替わり、警告が出ます。アプリはプレビューの警告にファミリーを表示し、`celesta-exporter` はファミリー・ウェイト・テキストレイヤーを出力します。OS のフォント管理で正確な名前を確認してください。すべての書き出し環境にインストールする手間を省くには、エントリーの隣にフォントを置き `<Font>` で読み込みます。

:::
:::toggle[口が動かない]

口パクには非圧縮 WAV と、母音を読めるかな・ローマ字の文字起こしが必要です。`prepare()` 内で `loadLipSync()` を実行し、トラックを `lipSync` として渡し、口のパスが PSD のレイヤーと完全一致するか確認します。PSD が何も表示しない場合は、PSDTool のプリセットで `layers` を指定します。

:::
:::toggle[エディタが @celesta.studio/react などの Celesta パッケージを解決できない]

Celesta でコンポジションを開き、**File → Set Up TypeScript** を選びます。`tsconfig.json` が `./.celesta/tsconfig.json` を継承しているか確認します。ソースビルドの場合はランタイムのインストール・型生成・ビルドも済ませてください。

`.celesta/` がコンポジションのあるフォルダーより上にある場合（コンポジションが `src/` にあり `.celesta/` がプロジェクトのフォルダーにある、モノレポでリポジトリのルートにある、など）、そのフォルダーをプロジェクトのルートにするには `celesta.config.json` が必要です。**File → Set Up TypeScript in Folder…** でそのフォルダーを選ぶと、Celesta は `celesta.config.json` がなければ空のものを置き、`.celesta/` を更新します。

:::
:::toggle[ランタイムの標準出力に書き込まれてコンポジションが失敗する]

Celesta は React コンポジションのランタイムと標準出力を通じてやり取りします。そのため、コンポジションやそれが読み込むパッケージが `process.stdout.write()` で直接書き込むと接続が壊れ、Celesta ランタイム以外が標準出力に書き込んだことを示すエラーで読み込みが止まります。ログには標準エラーに出力される `console.log()` や `console.error()` を使ってください。パッケージが自ら標準出力に書き込む場合は、出力を抑えるオプションやログの設定を確認してください。

:::
:::toggle[書き出しが unknown_preset や invalid_presets で失敗する]

`unknown_preset` は、`--preset`（MCP と TypeScript では `preset`）の名前のプリセットが、どのプリセットファイルにもないという意味です。Celesta は `--preset-file`、プロジェクトのルートにある `export-presets.json`、`settings.json` の隣にある自分のファイルの順に探します。名前の大文字と小文字は区別されます。メッセージが `the default preset … is in no presets file` のときは、プロジェクトのファイルの `"default"` が、どのファイルにもないプリセットを指しています。名前を直すか、`"default"` を消してください。`veryfast` のような名前は x264 のプリセットで、別配布の `ffmpeg-x264/libx264` エンコーダーの設定です。`ffmpeg-x264` をインストールしたうえで、`--video-encoder ffmpeg-x264/libx264 --video-option preset=veryfast` を使います。

`invalid_presets` は、書き出しが探す必要のあったプリセットファイルを読めないか、JSON として正しくないか、`version` が `1` でないという意味です。メッセージにファイルが表示されるので、直すか削除してください。プロジェクトのプリセットが先に見つかり、壊れているのが自分のファイルだけのときは、同じ名前のプリセットがあるか自分のファイルを確かめられなかったことを警告して、そのまま書き出します。`--preset-file` のファイルに同じ問題があるときは、ほかの引数と同じく引数の誤りとして拒否されます。[書き出しプリセット](/ja/docs/export/#書き出しプリセット)をご覧ください。

:::
:::toggle[MP4 の書き出しが始まらない]

サイズは 0 でない偶数にし、すべてのローカルメディアを利用可能にします。出力先が既存の場合は別のパスを使うか、CLI に `--overwrite` を明示します。

:::
:::toggle[指定した描画 API を使えない]

--driver auto を試すか、OS と GPU が対応している API を選んでください。DX12 は Windows、Metal は macOS 用です。GPU のない Linux では Mesa の lavapipe をインストールして --driver vulkan を使います。エラーに、ビルドに含まれていない API なのか、描画アダプタが見つからないのかが表示されます。

:::
::::

## まだ解決しませんか？

OS、再現手順、正確なエラーメッセージを添えて Issue を作成してください。可能なら小さなプロジェクトと、共有できるメディアも添えてください。

[GitHub で問題を報告 ↗](https://github.com/mika-f/Celesta/issues)
