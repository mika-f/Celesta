`@celesta.studio/aivis-speech` は、AivisSpeech の音声合成に使った AudioQuery と、合成された WAV から、各モーラ・音素・間が WAV のどこにあるかを求め、その時刻から LipSyncTrack を作ります。会話のコンポーネントと LipSyncTrack は `@celesta.studio/character` にあります。VOICEVOX の音声には [@celesta.studio/voicevox](/ja/docs/voicevox/) を使います。

AivisSpeech は VOICEVOX と同じ API を提供しますが、各音素の長さはモデルが決めるため、AudioQuery の長さはすべてダミーの 0 です。モーラの並び（ん・っ・句読点を含む）は AudioQuery から、その位置は WAV から求めます。声の開始と終了は正確で、その間の時刻は WAV の音量から推定するため、間はモデルが実際に作った無音に合わせて置かれます。

デスクトップアプリと CLI に同梱されています。ランタイムのビルド時にこのパッケージもビルドされ、File → Set Up TypeScript で型定義と読み込み設定が追加されます。非公開パッケージのため npm では配布していません。ブラウザのプレイグラウンドには対応していません。

## AudioQuery と WAV に合わせて口を動かす

WAV の生成に使った AudioQuery をそのまま使ってください。speedScale・prePhonemeLength・postPhonemeLength から WAV 内の声の位置を求めます。WAV は prepare() で読み込み、そのバイト列を渡します。このパッケージ自体は音声を合成せず、AivisSpeech サーバへの通信も行いません。AudioQuery を aivis-speech.tsx と同じフォルダに aivis-speech-query.json として保存し、以下の例の音声と口の画像は手元の素材に置き換えてください。

```tsx file=examples/aivis-speech.tsx title="aivis-speech.tsx"
```

**@celesta.studio/aivis-speech**

| API | 用途 |
| --- | --- |
| `lipSyncFromAivisSpeech(query, wav)` | AudioQuery と WAV（バイト列または decodeWav の結果）を LipSyncTrack に変換します。先頭の無音も含め、WAV の開始時点が 0 秒です。 |
| `lipSyncFromAivisSpeech(timing)` | 計算済みの時刻から同じトラックを作ります。口と時刻が食い違いません。 |
| `timingFromAivisSpeech(query, wav)` | すべてのモーラ・音素・間・アクセント句について、WAV の開始からの秒数を返します。形はクエリと同じで、accentPhrases[i].moras[j] が accent_phrases[i].moras[j] に対応します。声に合わせてモーラを点灯させたり、間で表情を変えたりするのに使います。 |

AivisSpeech では句読点も、母音が pau で text が正規化した記号（, . ! ? …）のモーラになります。句読点は moras に残り、文末の記号も含めて pauses にも並びます。モデルが間を置かなかった記号は短いか、長さが 0 になります。

どちらのエンジンのタイミングも `@celesta.studio/character` の `SpeechTiming` です。timingFromVoicevox は exact: true、timingFromAivisSpeech は exact: false を返します。モーラの点灯や間での表情の切り替えなど、タイミングを読むコードはどちらも受け取れ、`lipSyncFromSpeechTiming` でどちらからも口パクを作れます。

音素の時刻がわかっている VOICEVOX の音声（138 本、6 話者、3 種類の速度）で測ると、モーラの開始時刻の誤差は中央値 14 ms・90 パーセンタイル 71 ms で、口の形は 83% の時間で一致しました（loadLipSync は 51%）。推定は間を手掛かりにするため、途切れずに続く長い句の中ほどでは誤差が大きくなります。VOICEVOX の音声には、正確な timingFromVoicevox を使ってください。

AivisSpeechTiming、AivisSpeechAudioQuery、AivisSpeechAccentPhrase、AivisSpeechMora もエクスポートされます。SpeechTiming とその部品、speechVowelShape、LipSyncTrack は @celesta.studio/character から読み込みます。
