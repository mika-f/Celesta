Celesta は Remotion のようにパッケージに分かれています。コアの API は `@celesta/react` から、それ以外はパッケージ列のパッケージからインポートします。乱数・ノイズ・数学ヘルパーは `@celesta/math`（[ガイド](/ja/docs/math/)）、色付きコードは `@celesta/code`（[ガイド](/ja/docs/code/)）にあります。TypeScript 宣言で完全な props の型とエディタ補完を取得できます。

**レイヤー**

| API | パッケージ | 用途 |
| --- | --- | --- |
| `Composition` | `@celesta/react` | `width`、`height`、`fps`、`durationInFrames` を設定します。任意の `lang` で既定のテキスト言語を、`fontFallback="loaded"` で `<Font>` の書体を OS の書体より先に探すことを指定し、default export からひとつだけ返します。 |
| `Rect` | `@celesta/react` | `width`、`height`、任意の `fill`、`stroke`、`strokeWidth`、`cornerRadius` を持つ矩形。`dash`、`dashOffset`、`cap` で stroke を破線にできます。 |
| `Text` | `@celesta/react` | 文字列・数値の子を `style` で装飾し、`maxWidth` で折り返します。`lang` で言語を選び、`style.lang` はさらに優先されます。[テキストとフォント](/ja/docs/text-fonts/)を参照。 |
| `Group` | `@celesta/react` | 子のレイヤーに共通の位置・拡縮・回転・スキュー・3D 回転（`rotationX`、`rotationY`、`perspective`）・透明度を適用します。回転などの軸は自身の原点、または `originX`/`originY` で指定した点です。`lang` で既定のテキスト言語を、`fontFallback` でフォールバック先を、`clip` で矩形を、`mask` で任意の描画を指定して子の見える範囲を決められます。[図形とレイアウト](/ja/docs/layout/)を参照。 |
| `Mask` | `@celesta/react` | `Group` のマスクの読み方を決めます。`mode="luminance"` と `invert` を指定できます。 |
| `Image` / `Video` / `Audio` | `@celesta/react` | `src` でローカルファイルや HTTP(S) の素材を指定します。`Image` と `Video` は `width` と `height` で表示サイズを決められ、片方だけなら縦横比を保ちます。[画像・動画・音声](/ja/docs/media/)も参照してください。 |
| `defineShader` | `@celesta/shader` | WGSL のカスタムフィルタを定義します。パラメータを渡して呼び出し、結果を `shader` に渡します。 |
| `brightness`、`contrast`、`saturation`、`hue`、`grayscale`、`invert`、`tint`、`vignette` | `@celesta/shader` | `shader` の配列でカスタムシェーダーの間に入れる色補正です。同名の props でどのレイヤーにも指定できます。[レイアウトの「色補正」](/ja/docs/layout/)を参照してください。 |
| `pixelate`、`halftone`、`chromaticAberration`、`wave`、`noise`、`scanlines`、`zoomBlur`、`chromaKey` | `@celesta/shader` | `shader` の配列に入れるスタイライズのエフェクトとクロマキーです。同名の props でどのレイヤーにも指定できます。[レイアウトの「スタイライズ」「クロマキー」](/ja/docs/layout/)を参照してください。 |
| `Font` | `@celesta/react` | `src` でフォントファイルやウェブフォントの CSS を読み込み、`Text` で使えるようにします。[テキストとフォント](/ja/docs/text-fonts/)も参照してください。 |

**時間と動き**

| API | パッケージ | 用途 |
| --- | --- | --- |
| `Sequence` | `@celesta/react` | `from` と任意の `durationInFrames` で子の時間を配置します。`lang` で既定のテキスト言語を、`fontFallback` でフォールバック先も指定できます。 |
| `Loop` | `@celesta/react` | `from` から `durationInFrames` フレームごとに、`times` 回または囲む範囲の終わりまで子要素を繰り返します。周期ごとに子の時計、映像、音声が最初に戻ります。 |
| `FreezeFrame` | `@celesta/react` | 子要素を、コンポジション全体の `frame` 時点（絶対フレーム）の状態で、音声なしで描画します。 |
| `MotionBlur` | `@celesta/react` | フレームを中心に開いたシャッター（`shutterAngle`、既定 180）の間の `samples` 時点（既定 8）で子要素を描き、平均します。 |
| `Trail` | `@celesta/react` | 子要素の下に、`lagInFrames` ずつ古く薄い複製を `copies` 個描きます。 |
| `Transition` | `@celesta/transitions` | 親シーケンスの開始・終了に、フェード・スライド・拡縮を単独または組み合わせて適用します。 |
| `Series` / `computeSeries` | `@celesta/react` | 長さでシーンを続けて並べます。[シーン・キュー・拍](/ja/docs/motion-toolkit/)を参照。 |
| `defineScenes` / `Scenes` / `useScene()` / `sceneAt` | `@celesta/react` | 名前付きの章を続けて並べ、現在の章と進み具合を取得します。 |
| `<Composition chapters>` / `listChapters` | `@celesta/react` | エディタのプレビューで一覧表示し、シークできる名前付きの区間。シーンの一覧、会話の計画、区間の配列から作ります。[シーン・キュー・拍](/ja/docs/motion-toolkit/)を参照。 |
| `TransitionSeries` / `computeTransitionSeries` | `@celesta/transitions` | シーンの間をカット・クロスフェード・スライド・ワイプ・アイリス・クロックワイプ・ズーム・ディゾルブ・ぼかし・フリップ・キューブでつなぎ、重なりと全体の長さを計算します。[シーン・キュー・拍](/ja/docs/motion-toolkit/)を参照。 |
| `defineTransition` / `iris`、`clockWipe`、`zoom`、`dissolve`、`blur`、`flip`、`cube`、`crossfade`、`slide`、`wipe` | `@celesta/transitions` | `TransitionSeries` のトランジションを関数で定義したり、組み込みのトランジションのオプションを指定したりします。 |
| `Stagger` | `@celesta/react` | 各子を前の子から一定フレーム遅れて開始します。 |
| `progress` | `@celesta/react` | フレームの区間を、イージング付きの 0〜1 に制限した値にします。 |
| `useBeat()` / `beatAt` | `@celesta/react` | BPM のテンポから拍・小節・パルスを取得します。 |
| `useCue()` / `cueAt` | `@celesta/react` | `{ at, …data }` のリストから現在のキューを取得します。 |
| `TextReveal`, `useTypewriter()`, `useCountUp()` | `@celesta/text` | マスク付きの行表示、入力効果、数値カウンター。 |
| `Camera` | `@celesta/layout` | 大きなワールドの一部を、ズーム・回転・揺れ付きで映します。 |
| `Line` / `Polyline` / `Path` / `pointOnPolyline` | `@celesta/shapes` | 線分・曲線・塗りつぶした形。折れ線とパスは段階的に描けます。 |
| `pointOnPath` / `tangentOnPath` / `measurePath` / `getLength` / `getPointAtLength` / `getTangentAtLength` | `@celesta/shapes` | パスコマンド上の位置・向き・長さ。曲線も含みます。 |
| `interpolatePath` / `parseSvgPath` / `toSvgPath` | `@celesta/shapes` | コマンドの異なるパス間のモーフィングと、SVG の `d` 文字列との相互変換。 |
| `Circle` / `Ellipse` / `Arrow` | `@celesta/shapes` | 図のコンポーネント。円は丸い `Rect`、楕円と矢印は `Path` です。円と楕円は `Rect` と同様に配置します。 |
| `Triangle` / `Polygon` / `Star` / `Pie` / `Heart` | `@celesta/shapes` | `Path` で描く閉じた図形。角を丸められ、扇形の角度はアニメーションできます。 |
| `trianglePath` / `polygonPath` / `starPath` / `piePath` / `heartPath` | `@celesta/shapes` | 同じ図形のパスコマンド。`Path` に渡したり変形したりできます。 |
| `interpolate` / `Easings` | `@celesta/react` | フレームを、イージング・外挿付きの値に変換します。 |
| `interpolateColor` | `@celesta/react` | フレームを `#RRGGBBAA` の色に変換し、乗算済みアルファで混ぜます。 |
| `frameKeyframes` | `@celesta/react` | frameKeyframes(keys, { fps, origin? }) は、フレーム番号から音量・再生速度・ピッチのキーフレームを作ります。時刻は Audio を囲む Sequence の時計で (frame - origin) / fps 秒となり、origin の既定値は 0 です。音声のフェードなどに使えます。ブラウザでの書き出しは、音量と再生速度のキーフレームにも対応しますが、ピッチは使えません。 |
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
| `importVoiceFolder` | `@celesta/character` | 音声ソフトが書き出した `.wav` と `.txt` の組のフォルダーを台詞の列として読み込みます。詳しくは[キャラクターの台詞](/ja/docs/dialogue/)を参照してください。 |
| `matchVoiceLevels` / `analyzeVoiceLevel` / `scaleVolume` | `@celesta/character` | `prepare()` でローカルの WAV を測り、台詞の音量に掛けて声の大きさをそろえます。詳しくは[キャラクターの台詞](/ja/docs/dialogue/)を参照してください。 |
| `duckingVolume` / `speechIntervals` | `@celesta/character` | duckingVolume(plan, duck, { levels?, origin? }) は、台詞の plan の声の下で下がる BGM などの音量を作ります。`matchVoiceLevels` の report を渡すと、実際に声が出ている区間に合わせます。speechIntervals(plan, { levels? }) はその区間を秒で返します。音量の変化は `<Audio duck>` と同じです。 |
| `duckEnvelope` | `@celesta/media-utils` | duckEnvelope(intervals, duck, { origin? }) は、秒で表した区間から、ダッキングの音量の変化を `<Audio duck>` と同じ既定値でキーフレームとして返します。`origin` は秒で指定します（duckingVolume の `origin` はフレームです）。`duck.volume` で全体を拡大縮小できます。 |
| `Dialogue` | `@celesta/character` | 字幕を表示し、`audio` を再生し、有効な間の表情を変更します。行の途中の時刻で表情を切り替えることもできます。 |
| `expressionAt` | `@celesta/character` | 行の `expression`（名前、または `{ at, expression }` の切り替え）が、行の中のある時刻に選ぶ表情を返します。 |
| `loadLipSync` / `useLipSync` | `@celesta/character` | WAV から口の形を生成し、現在の形を取得します。 |
| `paginateSubtitles` | `@celesta/character` | 長い台詞の字幕を、実際の文字計測から時間指定のページに分けます。明示した区切りと読みの対応があれば音声タイミングに合わせ、なければ近似で配分します。台詞には `subtitlePages` と `emphasis` を手で書くこともできます。 |
| `loadDialogueLipSync` / `spokenTextOf` | `@celesta/character` | 並べる台詞に、ローカルの WAV と `spokenText`（字幕の `text` とは別に、実際に話す内容）から波形の口パクを付けます。 |
| `loadPsdPreset` | `@celesta/character` | PSDTool の `.pfv` のお気に入りから、表示レイヤーを読み込みます。 |

**レイアウト・データ・ツール**

| API | パッケージ | 用途 |
| --- | --- | --- |
| `TextBox` / `fitText` / `useFitText` / `measureText` / `useTextMetrics` | `@celesta/text` `@celesta/react` | 組版後の文字を測ったり、枠に収まる文字サイズを選んだりします。詳しくは[テキストとフォント](/ja/docs/text-fonts/)を参照してください。 |
| `emphasisContent` / `resolveEmphasis` / `sliceEmphasis` / `graphemes` | `@celesta/text` | データで渡す文字の一部を、書記素クラスタ単位の範囲と色・太さで強調し、`<Span>` として描きます。 |
| `phrases` / `phraseBoundaries` | `@celesta/text` | 日本語を、`lineBreak: 'phrase'` が一行に保つのと同じ BudouX のモデルで文節に分けます。 |
| `useTextMeasurer` | `@celesta/text` | `useTextMetrics()` が一つの文字を測るように、描画中にいくつもの文字を測ります。 |
| `resolveTimedRanges` / `timedRangeAt` | `@celesta/text` | 半開区間のフレーム範囲で表示するもの（字幕、字幕のページ）の空白・重なり・範囲外を検証し、あるフレームのものを探します。 |
| `FrameChrome` | `@celesta/components` | 四隅のブラケット・タイトル・タイムコード・現在の章・プログレスバーを、キャンバスの大きさに合わせて画面の周りに描きます。 |
| `PatternBackground` | `@celesta/components` | ドット・格子・ストライプ、または自前のマークを、キャンバスや指定した領域いっぱいに繰り返して描き、クリップします。フレームに合わせて動かせます。 |
| `Panel` | `@celesta/components` | 背景・枠線・角丸・影を持つ指定サイズの枠です。子は余白の内側に配置され、枠でクリップすることもできます。 |
| `Badge` | `@celesta/components` | 文字を計測した寸法の `Panel` に載せたラベルです。文字の前の装飾、`cornerRadius="pill"` による半円の両端、`maxWidth` での折り返しに対応します。 |
| `Counter` / `StatCard` / `formatNumber` | `@celesta/components` | 数えた数値を、固定の locale（既定 `'en-US'`）・小数桁・prefix / suffix・formatter で整形して描きます。単独でも、ラベルと補足を載せたカードでも使えます。 |
| `BarChart` / `Ranking` | `@celesta/components` | 時間差で伸びる横棒と、同じ進捗で数える数値です。`Ranking` は順位を加え、ソートするかどうかを明示します。 |
| `Callout` | `@celesta/components` | 計測したタイトルと本文で高さを決める注釈の枠と、同じ座標系の対象点へ向かう直線または折れ線の矢印です。 |
| `CaptionTrack` | `@celesta/components` | データだけで出す時間指定の字幕（`parseSubtitles()` の cue や自前の配列）。各フレームの字幕を帯に載せ、行数の上限まで折り返し、強調し、フェードで出し入れします。`render` で描き方を差し替えても cue の時計は変わりません。`highlight` は単語ごとのタイミングから話している語を色・背景帯・拡大で強調し、`paging` は語数・行数・文末・間で区切ったページを順に出します。日本語は文節単位でも区切れます。 |
| `MediaFrame` | `@celesta/components` | 画像または動画（`kind` で指定）を `Panel` の枠に contain / cover で収め、角丸・枠線・キャプションを付けます。動画の再生設定と音声はそのまま保たれます。 |
| `Motion` | `@celesta/components` | 一つの要素をフェード・移動・拡大で登場させ、書いたままの状態で保持し、退場させます。表示区間は外側の Sequence の区間か、自前で指定した区間です。シーン同士のつなぎには `TransitionSeries` を使います。 |
| `TitleCard` | `@celesta/components` | 章番号・キッカー・タイトル・副題を一つのブロックにまとめたタイトル画面です。キャンバスやセーフエリアの中で、左・中央・右と上・中央・下に置きます。タイトルは明示改行を保ち、収まるまで文字を小さくします。各要素は時間差で、またはまとめて登場します。 |
| `LowerThird` | `@celesta/components` | 名前と任意の肩書きを、文字に合わせた帯とアクセントの線で、キャンバスやセーフエリアの隅に表示するテロップです。長い行は折り返し、収まるまで文字を小さくします。帯が入ってから各行が続き、外側の Sequence の終わりで退場します。 |
| `TypewriterText` | `@celesta/components` | 文字を一文字ずつ入力して見せ、計測した組版の位置に線またはブロックのカーソルを置きます。フォントを問わず、`\n` や折り返しにも追従します。 |
| `Center`, `SafeArea`, `Stack`, `Grid`, `Fit`, `LayoutBounds` | `@celesta/layout` | 子のレイヤーを配置します。[レイアウトのヘルパー](/ja/docs/layout/)を参照。 |
| `preloadMedia` / `mediaDurationInFrames` | `@celesta/media-utils` | `prepare()` 内でメディアの長さとサイズを取得します。 |
| `loadAudioSpectrum` / `useAudioSpectrum` / `spectrumAt` | `@celesta/media-utils` | `prepare()` で音声ファイルを解析し、フレームごとの周波数帯と音量を読みます。[画像・動画・音声](/ja/docs/media/)を参照。 |
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
