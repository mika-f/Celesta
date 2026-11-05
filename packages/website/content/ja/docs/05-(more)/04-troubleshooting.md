::::accordion[トラブルシューティング]{multiple}
:::toggle[同梱ランタイムが見つからない]{open}

macOS ではアプリ全体を Applications にコピーしてから起動します。Windows のポータブル版は全体を展開し、`runtime` と DLL を `Celesta.exe` の隣に置きます。不足するファイルがある場合は、再展開または再インストールしてください。

:::
:::toggle[ソースビルドで FFmpeg が見つからない]

実行ファイルだけでなく、8.1.x の開発ライブラリが入っているか確認します。macOS ではコーデックプラグインをビルドするシェルで `PKG_CONFIG_PATH` を設定します。Windows は `VCPKG_ROOT` と MSVC ビルドツールを確認し、Linux は `PKG_CONFIG_PATH` に `/opt/ffmpeg8/lib/pkgconfig` などのビルド済み FFmpeg を指定します。[ソースビルドの準備](/ja/docs/build-from-source/)をご覧ください。

:::
:::toggle[動画や音声が表示されない、書き出しが no_codec で失敗する]

Celesta はメディアをコーデックプラグインで読み書きします。パッケージでは、Celesta に同梱のプラグインフォルダー（`Celesta.exe` の隣の `plugins\ffmpeg`、macOS アプリでは `Contents/PlugIns/ffmpeg`）に `ffmpeg` プラグインが残っているか確認するか、パッケージを入れ直してください。ソースビルドでは、リポジトリのルートで `node scripts/plugins/build.mjs plugins/ffmpeg` を実行し、Celesta を実行するシェルで `CELESTA_PLUGIN_PATH` に `plugins/dist` の絶対パスを設定します（`export CELESTA_PLUGIN_PATH="$PWD/plugins/dist"`）。[コーデックプラグイン](/ja/docs/plugins/#コーデックプラグイン)をご覧ください。

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
