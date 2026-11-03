`@celesta.studio/voicevox` は、VOICEVOX の音声合成に使った AudioQuery から、各モーラ・音素・間が WAV のどこにあるかを求め、その時刻から LipSyncTrack を作ります。ローカルの VOICEVOX Engine で台本全体を音声合成し、その音声をキャッシュすることもできます。会話のコンポーネント、WAV から作る口パク、LipSyncTrack は `@celesta.studio/character` にあります。変換機能は独立したパッケージから読み込んでください。@celesta.studio/character からは再エクスポートされません。

デスクトップアプリと CLI に同梱されています。ランタイムのビルド時にこのパッケージもビルドされ、File → Set Up TypeScript で型定義と読み込み設定が追加されます。非公開パッケージのため npm では配布していません。ブラウザのプレイグラウンドには対応していません。

## 音声合成の情報に合わせて口を動かす

WAV の生成に使った AudioQuery を、速度や間の設定も含めてそのまま使ってください。トラックは Dialogue や CharacterView の lipSync に渡すか、useLipSync() で読み取れます。これらの関数は音声を合成せず、VOICEVOX サーバへの通信も行いません。AudioQuery を voicevox.tsx と同じフォルダに voicevox-query.json として保存し、以下の例の音声と口の画像は手元の素材に置き換えてください。

```tsx file=examples/voicevox.tsx title="voicevox.tsx"
```

## 台本から音声を合成する

prepare() の中で prepareNarration(lines, options) を呼ぶと、起動中のローカルの VOICEVOX Engine が各台詞を読み上げます。audio、lipSync、narration を設定した台詞が返るので、そのまま planDialogue() に渡せます。Celesta はエンジンをインストールも起動もしません。たとえば `docker run --rm -p 127.0.0.1:50021:50021 voicevox/voicevox_engine:cpu-ubuntu24.04-latest` で先に起動してください。

```tsx
const lines = await prepareNarration([
  { id: 'intro', speaker: 'zunda', text: 'HPが1です。', spokenText: 'エイチピーがいちです。' },
  { id: 'reply', speaker: 'metan', text: '危ないわね。', expression: 'surprised' },
], {
  speakers: {
    zunda: { baseUrl: 'http://127.0.0.1:50021', styleId: 3 },
    metan: { baseUrl: 'http://127.0.0.1:50021', styleId: 2, query: { speedScale: 1.1 } },
  },
  cacheDir: './.celesta/voices',
});
plan = await planDialogue(lines, { fps: 30 });
```

音声はエントリファイルからの相対パスである cacheDir にキャッシュされます。読み上げる文、スタイル、クエリの設定、合成のフラグ、revision のいずれかが変わった台詞だけを合成し直します。spokenText を変えずに字幕だけを直したときや、表情・順番・間を変えたときは、同じ音声を使います。キャッシュ済みの台詞はエンジンを使わずにディスクから読むので、台本が変わっていなければエンジンを止めたままプレビューや書き出しができます。エンジンのユーザー辞書、エンジン本体、音声モデルの変更はキャッシュからは分からないため、変更したら revision を変えるか refresh を指定してください。audio を指定済みの台詞は合成しません。失敗すると、台詞・話者・失敗した段階を示す NarrationError が投げられます。リポジトリの packages/cli/examples/with-narration.tsx が、実行できる二人の会話の例です。

| オプション | 用途 |
| --- | --- |
| `speakers` | 話者キーごとの声：`baseUrl`、`styleId`（エンジンの `GET /speakers` で確認）、省略可能な `query` の設定（`speedScale`、`pitchScale`、`pauseLengthScale` など）、`interrogativeUpspeak`、`katakanaEnglish`、`revision`。台詞の `voice` でその台詞だけ上書きできます。 |
| `cacheDir` | 各台詞の WAV と最終的なクエリの保存先です。既定は `./.celesta/voices` です。 |
| `concurrency`、`timeout`、`signal` | 同時に合成する台詞の数（既定 2）、エンジンへの各リクエストの制限時間（ミリ秒、既定 60000）、中止に使う AbortSignal です。 |
| `revision`、`refresh` | すべての台詞を無効にするか、キャッシュに関係なく合成し直します（`true` または台詞の id のリスト）。 |

**@celesta.studio/voicevox**

| API | 用途 |
| --- | --- |
| `prepareNarration(lines, options)` | 台本の音声を合成・キャッシュし、planDialogue() に渡せる台詞を返します。 |
| `lipSyncFromVoicevox(query, options?)` | AudioQuery を LipSyncTrack に変換します。先頭の無音も含め、WAV の開始時点が 0 秒です。 |
| `lipSyncFromVoicevox(timing)` | 計算済みの時刻から同じトラックを作ります。口と時刻が食い違いません。オプションは timingFromVoicevox に渡してください。 |
| `timingFromVoicevox(query, options?)` | すべてのモーラ・音素・間・アクセント句について、WAV の開始からの秒数を返します。形はクエリと同じで、accentPhrases[i].moras[j] が accent_phrases[i].moras[j] に対応します。声に合わせてモーラを点灯させたり、間で表情を変えたりするのに使います。 |
| `voicevoxVowelShape(vowel)` | 音素を a/i/u/e/o/closed に対応付けます。大文字の無声化した母音も同じ形になり、N、cl、pau、sil、未知の値は closed になります。 |

**タイミングの設定**

| API | 用途 |
| --- | --- |
| `frameRate` | 既定は VOICEVOX の音素の時間刻みである 93.75 Hz です。丸めない場合は null、別のエンジンではその時間刻みを指定します。動画の fps とは別の設定です。 |
| `interrogativeUpspeak` | 既定は true です。疑問文の語尾上げを無効にして音声を合成した場合は false を指定してください。 |

NarrationError、NarrationLine、NarratedLine、NarrationAsset、NarrationStage、PrepareNarrationOptions、VoicevoxVoice、VoicevoxVoiceSettings、VoicevoxQuerySettings、VoicevoxAudioQuery、VoicevoxAccentPhrase、VoicevoxMora、VoicevoxTiming、VoicevoxTimedAccentPhrase、VoicevoxTimedMora、VoicevoxTimedPause、VoicevoxTimedPhoneme、VoicevoxSpan、VoicevoxTimingOptions、VoicevoxLipSyncOptions もエクスポートされます。LipSyncTrack は @celesta.studio/character から読み込みます。
