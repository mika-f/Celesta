台詞のシーンは 3 つのコンポーネントで構成され、React と JSON の両方で使えます。

- **キャラクター**は表情ごとの画像やレイヤー付き PSD の立ち絵、任意の口パク用の口、字幕の見た目を定義します。
- **キャラクタービュー**は立ち絵を画面に配置します。
- **台詞**は字幕を表示し、音声を再生し、表示中のビューの表情や口を変更できます。

## React で最初の会話を作る

以下のファイルの隣に、立ち絵 2 枚と音声 2 本を保存します。

```tsx file=examples/dialogue.tsx title="dialogue.tsx"
```

1. **キャラクターを宣言**する場所は `<Assets>` の内側です。ref を通じてビューから参照します。`subtitle` は `Text` と同じ位置・スタイルを取ります。
2. `<CharacterView>` で**ビューを配置**します。台詞が指定しない間は `defaultExpression` を表示します。ほかのレイヤーと同じく `x`、`y`、`scale`、アンカーを取ります。
3. `<Dialogue>` を `Sequence` で囲み、**各台詞の時間を指定**します。有効な間だけ字幕、音声、`expression` が適用され、その後ビューは既定の状態に戻ります。

> [!NOTE]
> **ひとつのビューに複数の台詞**
> `Dialogue` はキャラクターではなくビューを指すので、台詞を変えても同じ立ち絵が同じ場所に残ります。2 人の会話には、2 つ目の `Character` とビューを追加します。

`Audio` と同様に `volume`、`startFrom`、`playbackRate`、`muted` で台詞を調整します。`Dialogue` の `x`、`y`、`opacity` は字幕を移動・フェードさせ、`Transition` の中で便利です。

`Dialogue` の子の中で `<Span>` を使うと、台詞の一部を強調できます。`subtitle.render` には書いたままの台詞が `content` として渡されるので、`<Text style={style}>{content}</Text>` と描けば強調が保たれます。`text` は同じ台詞のただの文字列です。

## 自動の口パク

`loadLipSync()` は音声を分析し、発話区間に文字起こしの母音を配分して、各時点の口の形（`a`、`i`、`u`、`e`、`o`、`closed`）を生成します。`prepare()` から呼び出し、`Dialogue` または `CharacterView` に `lipSync` として渡します。

```tsx file=examples/lip-sync.tsx title="lip-sync.tsx"
```

- **音声**は非圧縮 WAV が必要です。母音を読めるよう、文字起こしはかなまたはローマ字で書いてください。漢字はスキップされます。
- **字幕と別の発話テキスト**：並べる台詞にかなの `spokenText` を付け（`text: 'HPが1です。'`、`spokenText: 'えいちぴーがいちです。'`）、`prepare()` で `loadDialogueLipSync(lines)` を呼びます。字幕と字幕ファイルには `text` が出て、口と音声合成エンジンは `spokenText` に従います。
- **PSD の立ち絵**は `face/mouth/a` のようなパスで口のレイヤーを指定します。立ち絵ファイルは全フォルダが非表示のことが多いので、`layers` にレイヤーパス配列、PSDTool の状態文字列、または `loadPsdPreset()` で読み込んだ `.pfv` のお気に入りを指定して表示レイヤーを選びます。
- **画像の立ち絵**も口パクできます。`portrait.lipSync` に、形ごとに立ち絵と同じサイズの透明な口画像を指定します。こうすると重ねたときに位置が揃います。
- 手動で口を動かすには、`CharacterView` または `Dialogue` に `mouth="a"`（または別の形）を渡します。`useLipSync(track)` で現在の形を取得し、別の場所で使えます。

## 会話を自動で並べる

prepare() で planDialogue(lines, { fps, gap, sceneLeadIn }) を呼ぶと、音声の長さを測って台詞を順に並べます。durationInFrames を指定した台詞は計測しません。gap と leadIn は秒、durationInFrames はフレーム単位です。各台詞には audio または正の durationInFrames が必要です。Composition の長さを plan から設定し、DialogueSeries の views で話者名と表示先を対応付けます。演出のタイミングには plan の line()、startOf()、range()、scene() を使えます。

長い台詞は、音声と口パクを 1 本のまま、字幕だけをページごとに表示できます。`subtitlePages`（台詞の開始からのフレームで `{ from, durationInFrames?, text }`、ページを並べると台詞の `text` になる）を書くか、`prepare()` で `paginateSubtitles(lines, { fps, style, maxWidth, maxLines })` に実際の文字計測から分けさせます。`emphasis`（書記素クラスタ単位の範囲と色・太さ、または話者の `subtitle.emphasisStyles` の名前）は、どのページに来ても語を強調します。`subtitle.render` にはページ、その強調、台詞と同じ時計が渡るので、帯の入場はやり直されません。

```tsx file=examples/dialogue-series.tsx title="dialogue-series.tsx"
```

holdThroughGap は、台詞の後の間も字幕と表情を保持します。holdSubtitle は、subtitle.render で描く字幕の背景を台詞の間も表示し、held でその状態を判別できます。subtitle.render には text、character（displayName を含む）、metrics、style、maxWidth、frame、durationInFrames が渡されるので、文字の大きさに合う字幕パネルを作れます。全体の例はリポジトリの with-dialogue-series.tsx を参照してください。

1 行の途中で表情を変えるには、`expression` に名前の代わりに `{ at, expression }` の切り替えのリストを渡します。`at` は `lipSync` と同じく、行の頭（音声の最初のサンプル）からの秒数です。最初の切り替えより前はビュー自身の表情、それ以降は直前の切り替えの表情になり、最後の表情は行の終わりまで（holdThroughGap では行間まで）続きます。`at` は 0 以上で、小さい順に並べます。秒数を手で書く代わりに、`timingFromVoicevox(query).pauses[0].start`（読点の間の始まり）のように声から取ると、音声を作り直してもずれません。ほかの部品から同じ表情を読むには `expressionAt(expression, seconds)` を使います。

```ts
{ id: 'psd-2', speaker: 'kiritan', audio: './voices/psd-2.wav', text: '…',
  expression: [{ at: 0, expression: 'smile' }, { at: timing.pauses[0].start, expression: 'smug' }] }
```

VOICEVOX や AivisSpeech で生成した音声には、WAV に文字起こしの母音を均等に割り振る代わりに、合成に使った AudioQuery と [@celesta.studio/voicevox](/ja/docs/voicevox/) または [@celesta.studio/aivis-speech](/ja/docs/aivis-speech/) を利用できます。

台本を VOICEVOX Engine に読み上げさせるには、planDialogue() の前に [@celesta.studio/voicevox](/ja/docs/voicevox/) の `prepareNarration()` を呼びます。各台詞を合成して音声をキャッシュし、`audio` と `lipSync` を設定するので、読み上げる文や声を変えた台詞だけが合成し直されます。

## 音声ソフトの書き出しフォルダーを読み込む

VOICEROID、A.I.VOICE、AquesTalk 系のソフトは、台詞ごとに `name.wav` と同じ名前のテキスト `name.txt` を保存できます。prepare() で `importVoiceFolder({ directory, speaker })` を呼ぶと、このフォルダーを planDialogue() に渡せる台詞の列にします。ID はファイル名、字幕はテキスト、長さは録音、口パクは波形から作られます。並び順はどの OS でも名前の自然順（`voice-2` の後に `voice-10`）です。

```tsx
const lines = await importVoiceFolder({ directory: './voices/akane', speaker: 'akane', manifest: './voices/akane/manifest.json' });
plan = await planDialogue(lines, { fps: 30 });
```

テキストは UTF-8 として読みます。`encoding: 'utf-16le'` や `'shift_jis'` も指定できます。読めないファイルは文字化けした字幕にせず、パス付きのエラーになります。マニフェスト（`{ "lines": [{ "file": "voice-1", "id", "speaker", "text", "spokenText", "scene", "expression", "gap" }] }`）を渡すと、ID、順番、話者、字幕、かなの発話テキスト、シーン、表情、間を上書きできます。口の母音はかなの `spokenText`、なければテキストから取ります。漢字を含み `spokenText` のない台詞は、警告を出して口の開閉だけにします。

開いている間に録音やテキストを保存し直す、追加する、削除すると、エントリーのフォルダーの外にあってもプレビューが再読み込みされ、長くなった台詞の後ろの台詞も後ろにずれます。書き込み途中のペアでは再読み込みが失敗し、保存が終わるまで直前の正しいプレビューを表示し続けます（`incomplete: 'skip'` を渡すと、そうした台詞を除いて読み込みます）。読めるのは非圧縮の PCM または float の WAV です。

## 声の大きさをそろえる

話者や音声合成エンジンが違うと、録音の大きさがかなり変わります。prepare() で planDialogue() の前に `matchVoiceLevels(lines, { targetRmsDbfs, peakCeilingDbfs, maxBoostDb, maxCutDb, speakers })` を呼び、その `lines` を渡します。ローカルの PCM/float WAV を 1 回だけ測り、目標に近づける一定のゲインを台詞の `volume`（数値でもキーフレームでもよく、時刻とイージングは保たれます）に掛けます。呼ばなければ何も変わりません。

測るのは、ゲート付きの発話 RMS（dBFS。-60 dBFS 未満の無音と、発話より 20 dB 以上小さい部分は除き、チャンネルごとに測ります）と、全チャンネルのサンプルピークです。調整しやすい最初の一歩であり、LUFS でも聞こえ方の大きさの一致を保証するものでもありません。ゲインは `maxBoostDb` / `maxCutDb` で制限され、`peakCeilingDbfs` を超えないように下げられます。無音やごく短い録音は 0 dB です。話者ごとに `speakers: { name: { offsetDb, targetRmsDbfs } }`、台詞ごとに `voiceLevel` を指定できます（`false` でそのまま、`{ offsetDb }` か `{ targetRmsDbfs }` で調整、`{ gainDb }` で固定ゲイン）。`matched.report` に台詞ごとの測定値、ゲイン、どの上限で止まったかが入ります。結果はファイルの内容でキャッシュされるので、WAV を差し替えると測り直します。

話している間だけ BGM やゲーム音を下げるには、`duck={{ under: 'voice' }}` を指定します。台詞の声は、`duckGroup` で別のグループ（`null` ならどこにも入らない）を指定しない限り `"voice"` グループに入っています。report で見つかった発声区間だけに合わせたいときは、代わりに `prepare()` で duckingVolume(plan, { volume, to }, { levels: report }) から `volume` を作ります。区間そのものは speechIntervals(plan, { levels }) で得られます。`packages/cli/examples/with-voice-level-matching.tsx` が全体の流れを示します。

## 実際の素材で確認

`packages/cli/examples/with-dialogue-series.tsx` は音声の長さに合わせた会話、`packages/cli/examples/with-lip-sync.tsx` は PSD 立ち絵の口パク、`packages/cli/examples/with-voice-folder.tsx` は音声フォルダーから台本を読むサンプルです。[GitHub](https://github.com/mika-f/Celesta) の **Code → Download ZIP** からソースを取得し、素材を含むアーカイブ全体を展開して、**File → Open…** で開いてください。ソースからのビルドは不要です。
