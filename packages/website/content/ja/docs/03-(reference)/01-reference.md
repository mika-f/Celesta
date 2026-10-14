Celesta は Remotion のようにパッケージに分かれています。コアの API は `@celesta/react` から、それ以外はパッケージ列のパッケージからインポートします。乱数・ノイズ・数学ヘルパーは `@celesta/math`（[ガイド](/ja/docs/math/)）、色付きコードは `@celesta/code`（[ガイド](/ja/docs/code/)）にあります。TypeScript 宣言で完全な props の型とエディタ補完を取得できます。

**レイヤー**

| API | パッケージ | 用途 |
| --- | --- | --- |
| `Composition` | `@celesta/react` | `width`、`height`、`fps`、`durationInFrames` を設定します。任意の `lang` で既定のテキスト言語を指定し、default export からひとつだけ返します。 |
| `Rect` | `@celesta/react` | `width`、`height`、任意の `fill`、`stroke`、`strokeWidth`、`cornerRadius` を持つ矩形。`dash`、`dashOffset`、`cap` で stroke を破線にできます。 |
| `Text` | `@celesta/react` | 文字列・数値の子を `style` で装飾し、`maxWidth` で折り返します。`lang` で言語を選び、`style.lang` はさらに優先されます。[テキストとフォント](/ja/docs/text-fonts/)を参照。 |
| `Group` | `@celesta/react` | 子のレイヤーに共通の位置・拡縮・回転・透明度を適用します。`lang` で既定のテキスト言語を、`clip` で矩形を、`mask` で任意の描画を指定して子の見える範囲を決められます。[図形とレイアウト](/ja/docs/layout/)を参照。 |
| `Mask` | `@celesta/react` | `Group` のマスクの読み方を決めます。`mode="luminance"` と `invert` を指定できます。 |
| `Image` / `Video` / `Audio` | `@celesta/react` | `src` でローカルファイルや HTTP(S) の素材を指定します。[画像・動画・音声](/ja/docs/media/)も参照してください。 |
| `defineShader` | `@celesta/shader` | WGSL のカスタムフィルタを定義します。パラメータを渡して呼び出し、結果を `shader` に渡します。 |
| `brightness`、`contrast`、`saturation`、`hue`、`grayscale`、`invert`、`tint`、`vignette` | `@celesta/shader` | `shader` の配列でカスタムシェーダーの間に入れる色補正です。同名の props でどのレイヤーにも指定できます。[レイアウトの「色補正」](/ja/docs/layout/)を参照してください。 |
| `Font` | `@celesta/react` | `src` でフォントファイルやウェブフォントの CSS を読み込み、`Text` で使えるようにします。[テキストとフォント](/ja/docs/text-fonts/)も参照してください。 |

**時間と動き**

| API | パッケージ | 用途 |
| --- | --- | --- |
| `Sequence` | `@celesta/react` | `from` と任意の `durationInFrames` で子の時間を配置します。`lang` で既定のテキスト言語も指定できます。 |
| `FreezeFrame` | `@celesta/react` | 子要素を、コンポジション全体の `frame` 時点（絶対フレーム）の状態で、音声なしで描画します。 |
| `Transition` | `@celesta/transitions` | 親シーケンスの開始・終了に、フェード・スライド・拡縮を単独または組み合わせて適用します。 |
| `Series` / `computeSeries` | `@celesta/react` | 長さでシーンを続けて並べます。[シーン・キュー・拍](/ja/docs/motion-toolkit/)を参照。 |
| `TransitionSeries` / `computeTransitionSeries` | `@celesta/transitions` | シーンの間をカット・クロスフェード・スライド・ワイプでつなぎ、重なりと全体の長さを計算します。[シーン・キュー・拍](/ja/docs/motion-toolkit/)を参照。 |
| `Stagger` | `@celesta/react` | 各子を前の子から一定フレーム遅れて開始します。 |
| `progress` | `@celesta/react` | フレームの区間を、イージング付きの 0〜1 に制限した値にします。 |
| `useBeat()` / `beatAt` | `@celesta/react` | BPM のテンポから拍・小節・パルスを取得します。 |
| `useCue()` / `cueAt` | `@celesta/react` | `{ at, …data }` のリストから現在のキューを取得します。 |
| `TextReveal`, `useTypewriter()`, `useCountUp()` | `@celesta/text` | マスク付きの行表示、入力効果、数値カウンター。 |
| `Camera` | `@celesta/layout` | 大きなワールドの一部を、ズーム・回転・揺れ付きで映します。 |
| `Line` / `Polyline` / `Path` / `pointOnPolyline` | `@celesta/shapes` | 線分・曲線・塗りつぶした形。折れ線は段階的に描けます。 |
| `Circle` / `Ellipse` / `Arrow` | `@celesta/shapes` | 図のコンポーネント。円は丸い `Rect`、楕円と矢印は `Path` です。円と楕円は `Rect` と同様に配置します。 |
| `Triangle` / `Polygon` / `Star` / `Pie` / `Heart` | `@celesta/shapes` | `Path` で描く閉じた図形。角を丸められ、扇形の角度はアニメーションできます。 |
| `trianglePath` / `polygonPath` / `starPath` / `piePath` / `heartPath` | `@celesta/shapes` | 同じ図形のパスコマンド。`Path` に渡したり変形したりできます。 |
| `interpolate` / `Easings` | `@celesta/react` | フレームを、イージング・外挿付きの値に変換します。 |
| `interpolateColor` | `@celesta/react` | フレームを `#RRGGBBAA` の色に変換し、乗算済みアルファで混ぜます。 |
| `frameKeyframes` | `@celesta/react` | frameKeyframes(keys, { fps, origin? }) は、フレーム番号から音量や再生速度のキーフレームを作ります。時刻は Audio を囲む Sequence の時計で (frame - origin) / fps 秒となり、origin の既定値は 0 です。音声のフェードなどに使えます。ブラウザでの書き出しは、固定の音量と再生速度に限られます。 |
| `spring` | `@celesta/react` | 物理モデルで 0 から 1 に収束する値。 |
| `useCurrentFrame()` / `useCurrentTime()` | `@celesta/react` | 親シーケンスを基準にした現在のフレームまたは正確な時刻。 |
| `useVideoConfig()` | `@celesta/react` | コンポジション（またはシーケンス）のサイズ・フレームレート・長さ。 |
| `timecodeToFrame()` / `frameToTimecode()` | `@celesta/react` | `MM:SS.mmm` 形式をフレーム番号に変換し、フレームを `HH:MM:SS:FF` に整形します。 |

**キャラクター**

| API | パッケージ | 用途 |
| --- | --- | --- |
| `Assets` / `Character` | `@celesta/react` `@celesta/character` | 立ち絵・表情・口パクの口・字幕のスタイルを宣言します。 |
| `CharacterView` | `@celesta/character` | キャラクターの立ち絵を配置します。`expression`、`mouth`、`lipSync` を取ります。 |
| `DialogueSeries` / `planDialogue` | `@celesta/character` | 音声の長さをもとにタイミングを計算し、会話を順に描画します。詳しくは[キャラクターの台詞](/ja/docs/dialogue/)を参照してください。 |
| `Dialogue` | `@celesta/character` | 字幕を表示し、`audio` を再生し、有効な間の表情を変更します。 |
| `loadLipSync` / `useLipSync` | `@celesta/character` | WAV から口の形を生成し、現在の形を取得します。 |
| `loadPsdPreset` | `@celesta/character` | PSDTool の `.pfv` のお気に入りから、表示レイヤーを読み込みます。 |

**レイアウト・データ・ツール**

| API | パッケージ | 用途 |
| --- | --- | --- |
| `TextBox` / `fitText` / `useFitText` / `measureText` / `useTextMetrics` | `@celesta/text` `@celesta/react` | 組版後の文字を測ったり、枠に収まる文字サイズを選んだりします。詳しくは[テキストとフォント](/ja/docs/text-fonts/)を参照してください。 |
| `Center`, `SafeArea`, `Stack`, `Grid`, `Fit` | `@celesta/layout` | 子のレイヤーを配置します。[レイアウトのヘルパー](/ja/docs/layout/)を参照。 |
| `preloadMedia` / `mediaDurationInFrames` | `@celesta/media-utils` | `prepare()` 内でメディアの長さとサイズを取得します。 |
| `parseSubtitles` / `formatSubtitles` / `Subtitles` | `@celesta/react` | SRT・WebVTT をフレーム単位の字幕に読み込み、書き出します。`Subtitles` は `--subtitles` で書き出す字幕を宣言します。[動画の書き出し](/ja/docs/export/)を参照。 |
| `DebugOverlay` / `DebugBounds` / `useDebugGuides()` / `useIsPreview()` | `@celesta/debug` `@celesta/react` | プレビューと、--debug-guides を付けた PNG 書き出しに描かれるデバッグ用のガイド。動画には含まれません。 |

## 旧 JSON プロジェクト用 API

次の API は既存の JSON タイムラインとの互換性のために残っています。新しいコンポジションでは、React コンポーネントと通常の props を直接使えます。

**旧 JSON プロジェクト用 API**

| API | パッケージ | 用途 |
| --- | --- | --- |
| `loadProject`, `ProjectProvider`, `useProjectProperty` | `@celesta/project` | JSON プロジェクトから値を読みます。 |
| `ProjectTimeline` / `ProjectTrack` | `@celesta/project` | 対応する JSON のタイムライン全体、または 1 トラックを描画します。 |
| `registerComponent` / `defineProjectProperties` | `@celesta/react` | JSON プロジェクトや Inspector でコンポーネント・プロパティを使えるようにします。 |
