# DELTA

Celesta のリリースごとに「前のバージョンから何が変わったか」をまとめるモーショングラフィックのシリーズです。
インク黒とオレンジ、Bebas Neue の見出し、Space Grotesk の本文、JetBrains Mono のコードで、
各機能をリリースノートの 1 ページのように見せます。見出し、何ができるか、それを使う差分、マージされた PR、そしてその場で動くデモです。

- `film-0.7.0.tsx` — v0.6.0 → v0.7.0 の 91 秒（エントリ）。Celesta の File → Open… で開きます。
- `shorts-0.7.0.tsx` — 同じ 91 秒を 1080×1920 の縦長に組み直したショート版（エントリ）。
- `film-0.6.0.tsx` — v0.5.0 → v0.6.0 の 62 秒。
- `components/` — シリーズ共通の見た目。配色・書体（`theme.ts`）、章のページ（`ChapterShell.tsx`）、右上の進行表示（`Hud.tsx`）、差分パネル（`DiffPanel.tsx`）、WGSL シェーダー（`shaders.ts`）、背景のドット（`Backdrop.tsx`）、Δ マーク（`Delta.tsx`）。0.7.0 は BGM のメーター付きの `Hud.tsx` を `v0.7.0/` に持ちます。
- `v0.7.0/`・`v0.6.0/` — 各リリースの中身。冒頭（`Open.tsx`）、章の一覧（`chapters.tsx`）とデモ（`demos/`）、アウトロ（`Outro.tsx`）、リリースノートの PR 一覧（`prs.ts`）、テンポと章の位置（`timeline.ts`）。
  0.7.0 は 2 つのエントリが同じカット（`reel.tsx`）を、それぞれのキャンバス（`layout.ts`）に組んで再生します。
- `make-score-<version>.py` — BGM を生成する Python スクリプト。標準ライブラリのみ。
- `poster-0.7.0.jpg`・`poster.jpg` — 0.7.0 は `poster` コンポジションの書き出し、0.6.0 は映像のアウトロから抽出した静止画。

次のリリースは `film-<version>.tsx` と `v<version>/` を足し、`components/` を使い回します。
`components/` は v0.6.0 の API だけで書かれており、0.6.0 の映像はそのまま v0.6.0 で動きます。

![poster](poster-0.7.0.jpg)

## 構成（v0.7.0）

1920×1080、30 fps、180 BPM のドラムンベース（1 拍 = 10 フレーム、1 小節 = 40 フレーム）。
12 の章は 4 小節ずつ、続く MORE は 1 小節に 1 枚のカードが 8 枚です。切り替えはすべて小節の頭に落ちます。

| 時間 | 内容 |
| --- | --- |
| 0–2.7 秒 | 白い閃光のあと、v0.7.0 までにマージされた 47 本の PR が `<MotionBlur>` をかけて流れ、右の番号が #220 から #361 まで進む |
| 2.7–4 秒 | Δ が破線で描かれ、拍ごとに星・ハート・六角形へ `interpolatePath()` で形を変えながら `<Trail>` を引く |
| 4–5.3 秒 | 「0.7.0」の文字をマスクにして、これから出てくる章が 1 拍ずつ文字の中に映る |
| 5.3–8 秒 | オレンジの DELTA カード。WGSL のグリッチと組み込みの色収差・走査線を 1 つの `shader` 配列で重ね、最後の無音の拍で反転してズームブラーがかかる |
| 8–13.3 秒 | Δ01 TRANSITIONS — プレビューの中の本物の `TransitionSeries` が 2 拍ごとに新しい 7 種で切り替わる |
| 13.3–18.7 秒 | Δ02 MASK — MASK の文字の中で DELTA カードが再生され、ぼかした円の輝度マスクがスポットライトになり、最後に反転する |
| 18.7–24 秒 | Δ03 MOTION BLUR — 同じ車輪と Δ をブラーなしと `<MotionBlur>` で並べ、跳ねる星に `<Trail>` |
| 24–29.3 秒 | Δ04 EFFECTS — 同じ絵にハーフトーン・ピクセル化・色収差・波・ノイズ・走査線・ズームブラーを 2 拍ずつかけ、最後にクロマキー |
| 29.3–34.7 秒 | Δ05 COLOR — 色補正のプロパティ 8 種を並べ、続いて `shader` 配列を 1 段ずつ積む |
| 34.7–40 秒 | Δ06 BLEND — 17 のブレンドモードを同じ背景と円で並べる |
| 40–45.3 秒 | Δ07 SHAPES — Triangle・Polygon・Star・Pie・Heart の角が拍に合わせて丸まり、破線と点線が流れる |
| 45.3–50.7 秒 | Δ08 PATHS — SVG のパスを `progress` で描き、先端の矢印が接線に沿って進む。下では輪郭が拍ごとにモーフィングする |
| 50.7–56 秒 | Δ09 SPECTRUM — いま鳴っているこの映像の BGM の 32 バンドと音量、直近 3 秒のスペクトログラム |
| 56–61.3 秒 | Δ10 MEDIA — 音の鳴る `<Video>` とループ、時間に合わせて進むアニメーション画像、透過の書き出し |
| 61.3–66.7 秒 | Δ11 MCP — エージェントが `render_frames` でこの映像の 3 フレームを受け取り、`inspect` する |
| 66.7–72 秒 | Δ12 EDITOR — この映像のクリップと BGM の波形が並ぶタイムライン、ピンチでのズーム、ソースを開く、Problems タブ |
| 72–82.7 秒 | MORE — COMPOSITIONS / GOOGLE FONTS / SUBTITLES / SAFE AREAS / AIVISSPEECH / REACT 19 / WEBSITE / FIXES |
| 82.7–90.7 秒 | BGM のスペクトルが輪になって脈打つ Δ、タグライン、13 場面のサムネイル。最後の和音とベルが鳴り切って終わる |

BGM は 0.6.0 の 120 BPM のハウスから変え、映画予告風のブラスとサブベースで始まるドラムンベースにしました。
F マイナー（Fm – D♭ – B♭m – C）の 2 ステップにリースベースを乗せ、SHAPES と PATHS の 2 章だけ軽く落とし、
SPECTRUM からはリードを足して 2 回目のドロップにしています。MORE の後半はドラムを抜いてスネアのロールで盛り上げます。

## 新機能で作っている部分（v0.7.0）

- 章の切り替え 14 か所に同じものはありません。組み込みの 11 種すべて（0.7.0 で入った iris・clock-wipe・zoom・dissolve・blur・flip・cube を含み、iris と zoom は開く向きと閉じる向きの両方）と、
  `defineTransition()` で定義した 2 種（冒頭から Δ01 への Δ 形のマスク、SPECTRUM への色収差とピクセル化のグリッチ）です（`v0.7.0/cuts.tsx`）。
- 冒頭の PR ログは `<MotionBlur>`、モーフィングする Δ は `<Trail>`・`interpolatePath()`・破線の `<Path progress>`、
  「0.7.0」は `<Group mask>` で、文字の中の章は `<FreezeFrame>` です。
- DELTA カードと MORE のカードは、WGSL のシェーダーと組み込みの `chromaticAberration`・`scanlines`・`pixelate` を 1 つの `shader` 配列で重ねています。
- `prepare()` で BGM を `loadAudioSpectrum()` に通し、右上のメーター（`useAudioSpectrum()`）、冒頭の色収差、SPECTRUM 章、
  エディタ章の波形、アウトロの輪が同じ解析結果で動きます。
- 欧文の 3 書体は `<Font google="…">` で名前から読み込み、和文は使う文字だけの Noto Sans JP です。
- 各場面の字幕を日本語と英語の `<Subtitles lang>` で宣言しており、`--subtitles` で SRT と WebVTT として書き出せます（画面には描きません）。
  パスに `{lang}` を入れると言語ごとのファイルになるので、1 回の書き出しで 2 言語 × 2 形式の 4 ファイルが揃います。
- エントリには `film` と `poster` の 2 つのコンポジションがあります。`poster` はアウトロの 1 フレームを `<FreezeFrame>` で描いたものです。
- MCP 章のフレーム、エディタ章のプレビュー、MASK 章のカード、アウトロのサムネイルは、この映像そのものを `<FreezeFrame>` で描き直したものです。

## ショート版（v0.7.0）

`shorts-0.7.0.tsx` は本編と同じ 91 秒・同じ BGM・同じ切り替えを 1080×1920 で再生する縦長版です（Shorts、TikTok、Reels 向け）。

- **組み直した部分**: 冒頭・MORE・アウトロは、本編と同じ部品がキャンバスの大きさに合わせて組み直されます。
  章のページは縦用の `ShortChapter.tsx` で、ラベル・タイトル・見出し・差分を上に、デモ（本編の 860×742 のステージを 0.95 倍）を下に大きく置きます。
  本文の説明は省き、PR 番号はプレビューの見出しに移しています。
- **セーフエリア**: 読ませる文字は `'vertical-social'`（上 192・右 200・下 384・左 72 px）の内側に置いています。
  ただし、タイトルが入ってくるときの字間や、デモの枠の端は少しはみ出します。
  中央に置くもの（冒頭の図形や DELTA カード、アウトロ）は、セーフエリアではなく画面の中心に揃えています。
- **セーフエリアの外側**: プラットフォームの UI に隠れる縁は `ShortFrame.tsx` が埋めます。
  下端には BGM のバンドが帯になって伸び縮みし、右下には章番号が大きく画面からはみ出して置かれ、右端を章名が縦に流れます。
- **本編の再利用**: リプレイやサムネイル（冒頭のティザー、MASK、MCP、EDITOR、SUBTITLES、アウトロ）は横長の本編のフレームです。
- **補助線**: `<DebugOverlay safeArea="vertical-social" />` を入れてあり、アプリのプレビューと `--debug-guides` 付きの PNG にだけ描かれます（MP4 には描かれません）。

## 本物と演出（v0.7.0）

- **実際の機能**: 上に挙げたもののほか、TRANSITIONS の再生、MASK の文字マスクと輝度マスク、MOTION BLUR と Trail、EFFECTS と COLOR の各フィルターと
  シェーダー配列、BLEND の 17 モード、SHAPES の図形と破線、PATHS の計測・トリム・モーフィング・SVG パス、SPECTRUM の値、
  MORE の SUBTITLES の見本（SPECTRUM 章に合わせた英語の SRT を `parseSubtitles()` で読み、4 倍速で再生した SPECTRUM 章に載せたもの）と SAFE AREAS の数値（`safeAreaInsets()`）は実際の描画と計算です。
- **説明のための描画**: MEDIA の 3 つのファイル（動画、APNG、透過の書き出し）は実ファイルではなく、似せて描いたものです。
  動画の波形はこの映像の BGM の音量、スピナーのコマは時間から求めています。
  MCP の会話、EDITOR のエディタ画面・ピンチ・Problems タブのエラー文、AIVISSPEECH のモーラ、WEBSITE のページも同様に描いたもので、
  MCP の引数名と `inspect` の結果のフィールド名・値（`compositions` の中身は省略）、EDITOR のソースの行番号（`ColorDemo.tsx:62`）はこのリポジトリの実際のものです。
- **数値**: SAFE AREAS の数値は `'vertical-social'` を 1080×1920 で解決した値、MCP の `durationInFrames` はこの映像の長さです。

## 構成（v0.6.0）

1920×1080、30 fps、120 BPM（1 拍 = 15 フレーム、1 小節 = 60 フレーム）。章は 2 小節ずつで、切り替えは小節の頭に落ちます。

| 時間 | 内容 |
| --- | --- |
| 0–2 秒 | v0.6.0 のリリースノートに載った 60 本の PR が流れ、右の番号が #168 から #273 まで進む |
| 2–4 秒 | ログが 7,000 個の粒子にほどけ、Δ に集まる |
| 4–6 秒 | Δ が画面を覆うオレンジに膨らみ、DELTA がグリッチしながら定まる |
| 6–10 秒 | Δ01 SHADER — 自作 WGSL の ripple / glitch / pixelate がその場で動く |
| 10–14 秒 | Δ02 INSTANCES — 30,000 粒子の円盤が Δ の輪郭に折りたたまれる |
| 14–18 秒 | Δ03 TRANSITIONS — 本物の `TransitionSeries` の縮小再生と、`computeTransitionSeries()` のタイムライン |
| 18–22 秒 | Δ04 SPAN — 1 つの Text の中で語ごとに色・太さ・書体が変わり、折り返し幅が動いても 1 段落のまま |
| 22–26 秒 | Δ05 FONT STACK — 欧文と和文を 1 つのスタックで組み、`letterSpacing: { em }` を 3 サイズで比べる |
| 26–30 秒 | Δ06 CODE — `highlightCode()` の行が 1 行ずつ入り、行の color run を一覧する |
| 30–34 秒 | Δ07 FREEZE — この映像そのものを Δ01 まで巻き戻して再生し、過去の 5 場面を並べる |
| 34–38 秒 | Δ08 LOOP — `loopNoise` で動く 120 フレームのリングと `inspect.mjs --loop` の結果 |
| 38–42 秒 | Δ09 10-BIT — 暗いグラデーションのバンディングとディザ、#252 の計測値、`--bit-depth 10` |
| 42–46 秒 | Δ10 TEMPLATES — `--init --template`、Properties パネル、`--props` で同じテンプレートの値を差し替える |
| 46–50 秒 | Δ11 PACKAGES — `@celesta/react` から 12 のパッケージが立ち上がり、codemod が import を移す |
| 50–54 秒 | Δ12 SPEED — 各 PR の before / after |
| 54–62 秒 | Δ、タグライン、12 章のサムネイル。最後の和音が鳴り切って終わる |

## 新機能で作っている部分（v0.6.0）

紹介している機能の多くは、この映像自体の組み立てにも使っています。

- 章の切り替えは 1 つの `TransitionSeries`（`wipe`）。章の頭は `computeTransitionSeries()` の結果から求め、`timeline.ts` と一致するかを読み込み時に確かめます。
- 背景のドットは `<Static>` の中の `<Instances>`、冒頭の粒子と INSTANCES 章も `<Instances>` です。
- 差分パネルは `highlightCode()` の行と color run を自前で描いています。
- 章タイトルは `useFitText()` で幅に合わせ、`letterSpacing: { em }` で字間を詰めながら入ります。
- 見出しの強調は `<Span>`、和文を含む文字はすべて `fontFamily` のスタック（`[欧文, 'Noto Sans JP']`）で組んでいます。
- DELTA カードのグリッチ、SHADER 章の 3 種は `defineShader` の WGSL です。
- FREEZE 章とアウトロのサムネイルは、この映像そのものを `<FreezeFrame>` で描き直したものです。
- 使っている API はすべて v0.6.0 で公開されているものです（色フィルタや `shader` の配列など、その後に入ったものは使っていません）。

## 本物と演出（v0.6.0）

- **実際の機能**: 上に挙げたもののほか、SHADER のシェーダー、TRANSITIONS の再生とタイムライン、SPAN の再折り返し、
  FONT STACK の和欧混植と em 指定の字間、CODE の行と run、FREEZE の巻き戻し、LOOP のリングの周期性はすべて実際の描画です。
- **説明のための描画**: 10-BIT の左半分のバンディングは段差を手で描いた図解で、右半分が実際のグラデーションです。
  TEMPLATES の Properties パネルとランキングのカードはアプリ画面やテンプレートの出力ではなく、似せて描いたものです。
  LOOP と TEMPLATES と PACKAGES の端末表示は、v0.6.0 のコマンドと `inspect.mjs --loop` の出力形式に合わせた文面です。
- **数値**: SPEED と 10-BIT の数値は各 PR に書かれた計測値です（いずれも同じマシンでの before / after。
  #259・#272・#180・#266 は Apple M4、#189 は Windows）。計測の対象が違うため、棒は組ごとに別の尺度です。

## 再生成

WAV と MP4 はリポジトリ全体の設定で Git の管理対象外です。クローン後は最初にスコアを生成してください。
0.7.0 は `prepare()` で BGM を解析するため、WAV がないと読み込みで失敗します。

```sh
python3 examples/delta/make-score-0.7.0.py
node skills/celesta/scripts/inspect.mjs examples/delta/film-0.7.0.tsx --frames 0,1360,2719
Celesta-export --json --react examples/delta/film-0.7.0.tsx examples/delta/delta-0.7.0.mp4 --subtitles 'examples/delta/delta-0.7.0.{lang}.srt' --subtitles 'examples/delta/delta-0.7.0.{lang}.vtt'
Celesta-export --json --react --composition poster examples/delta/film-0.7.0.tsx --frame 0 examples/delta/poster-0.7.0.png
```

1 本目の書き出しで、映像と一緒に `delta-0.7.0.ja.srt`・`delta-0.7.0.en.srt`・`delta-0.7.0.ja.vtt`・`delta-0.7.0.en.vtt` が書き出されます。
`{lang}` は `<Subtitles lang>` の言語に置き換わります。シェルが `{lang}` を展開しないよう、パスは引用符で囲みます。
字幕だけを書き直すときは、映像のパスを省くと描画せずに字幕ファイルだけを書き出します。

```sh
Celesta-export --json --react --overwrite examples/delta/film-0.7.0.tsx --subtitles 'examples/delta/delta-0.7.0.{lang}.srt' --subtitles 'examples/delta/delta-0.7.0.{lang}.vtt'
```

ショート版の書き出しと、セーフエリアの補助線付きのコンタクトシートです。

```sh
Celesta-export --json --react examples/delta/shorts-0.7.0.tsx examples/delta/delta-0.7.0-shorts.mp4
Celesta-export --json --react --debug-guides --every 80 --contact-sheet --columns 6 examples/delta/shorts-0.7.0.tsx examples/delta/shorts-sheet.png
```

```sh
python3 examples/delta/make-score-0.6.0.py
node skills/celesta/scripts/inspect.mjs examples/delta/film-0.6.0.tsx --frames 0,900,1859
Celesta-export --json --react examples/delta/film-0.6.0.tsx examples/delta/delta-0.6.0.mp4
```

ソースから実行する場合は `Celesta-export` を `cargo run -p celesta-exporter --release --` に置き換えます。
フォントは初回に Google Fonts から取得され、以降はキャッシュされます。
