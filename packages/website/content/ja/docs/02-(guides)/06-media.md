`Image`、`Video`、`Audio` は `src` からファイルを読み込みます。相対パスはコンポジションのファイルが基準なので、プロジェクト内でメディアを隣に置いてください。

`src` には `http://` または `https://` の URL も指定できます。ネイティブのプレビュー、書き出し、`preloadMedia()` は初回にダウンロードし、ローカルにキャッシュします。以後は更新確認をせずに再利用するため、内容が変わった場合は URL を変更するかキャッシュを削除してください。ブラウザと同じく、ダウンロードの前に URL を正規化します。パスとクエリの空白や ASCII 以外の文字はパーセントエンコードし、ASCII 以外のドメインは ASCII の表記（punycode）に変換し、`#` 以降のフラグメントは取り除きます。この違い、またはスキームやドメインの大文字小文字だけが違う URL は、同じキャッシュを使います。

```tsx file=examples/media.tsx title="media.tsx"
```

- `Image` と `Video` は、`width` か `height` を指定しなければ自身のピクセルサイズで描画されます。片方だけ指定すると絵の縦横比を保ち、両方指定すると `Video` はその矩形に合わせて伸縮します。`Image` も同じですが、`fit="contain"` なら矩形に収まるように拡縮し、`"cover"` なら矩形を覆うように拡縮して、はみ出す部分を切り取ります。`x` / `y` とアンカーで配置します。`scale` や `Fit` でも、ほかのレイヤーと同じように拡縮できます。画像は PNG、JPEG、WebP、GIF、PNM に対応します。
- アニメーション GIF・APNG・アニメーション WebP は、親シーケンスの時計に合わせて各フレームをその表示時間どおりに描き、ループします。`startFrom` と `playbackRate` は `Video` と同じように使えます。`loop={false}` で最後のフレームのまま止まります。`preloadMedia()` は尺を `animation.durationSeconds` で返します。
- `Video` と `Audio` は `startFrom`（ファイル内の開始秒）、`playbackRate`、`volume`、`muted` を取ります。`Video` は動画の音声も鳴らすので、音を聞くために同じファイルの `Audio` を並べる必要はありません。映像だけにするときは `muted` を指定します。
- `Video` と `Audio` に `loop` を指定すると、再生がファイルの終わりに達したところで先頭から繰り返します。`Video` は映像と音声がそろって戻ります。指定しなければ、最後のフレームのまま止まり、音は鳴りやみます。
- `Sequence` 内のメディアはシーケンスの開始と同時に始まり、終了時に止まります。
- `volume` と音声の `playbackRate` には数値またはキーフレームを使えます。時刻は親シーケンスの開始からの秒数で、`{ value, timescale }` の形式です。

frameKeyframes(keys, { fps, origin? }) は、フレーム番号から音量・再生速度・ピッチのキーフレームを作ります。時刻は Audio を囲む Sequence の時計で (frame - origin) / fps 秒となり、origin の既定値は 0 です。声の下で音楽を下げるなど、フェードでは表せない音量の変化に使えます。ブラウザでの書き出しは、音量と再生速度のキーフレームにも対応しますが、ピッチは使えません。

## 枠に入れて表示する

`@celesta/components` の `MediaFrame` は、画像や動画を指定した `width` × `height` の枠に収めます。枠は `Panel` が描き（背景、メディアの上に描く枠線、角丸、影、`padding` による余白）、下に任意の `caption` を付けられます。メディアの種類は `kind`（`"image"` または `"video"`）で明示し、ファイル名からは推測しません。`fit="contain"`（既定）は全体を中央に表示し、`"cover"` は枠を埋めてはみ出す部分を切り取ります。`Video` には `fit` がないので、`MediaFrame` は動画を拡縮して収めます。そのため、動画には大きさを表す `sourceWidth` と `sourceHeight` も必要です。`prepare()` で `preloadMedia()` を 1 回呼んで取得してください。`startFrom`、`playbackRate`、`volume`、`muted`、`loop` はそのまま `Video` に渡るので、`Sequence` と同時に始まり、音声も単独の `Video` と同じに鳴ります。現在の装飾は plain だけです。

```tsx title="角丸の枠に入れた動画"
let clip: MediaInfo;
export async function prepare() { clip = await preloadMedia("./demo.mp4"); }

<MediaFrame kind="video" src={clip.src} sourceWidth={clip.video!.width} sourceHeight={clip.video!.height}
  width={960} height={540} fit="cover" cornerRadius={24} caption="アプリのプレビュー" />
```

## フェードイン・フェードアウト

`fadeIn` と `fadeOut` は秒数を取ります。クリップは聞こえ始めてから `fadeIn` 秒かけて無音から立ち上がり、聞こえなくなる前の `fadeOut` 秒で無音まで下がります。

```tsx
<Sequence durationInFrames={300}>
  <Audio src="bgm.mp3" volume={0.6} fadeIn={1.5} fadeOut={3} />
</Sequence>
```

- フェードの位置は、クリップが聞こえ始める位置と聞こえなくなる位置で決まります。つまり `Sequence` の両端か、それより短く切る外側の `Sequence` の両端です。シーケンスの長さを変えると、フェードアウトもその終わりに追従します。
- フェードは `volume` に掛け合わされます。`volume` が数値でもキーフレームでも同じで、置き換えはしません。
- フェードは振幅に対して直線的に変化します。ほかのカーブはまだ選べません。
- `fadeIn` と `fadeOut` の合計がクリップより長いときは、2 つが重なって掛け合わされるので、最大音量には届きません。
- フェードはファイルではなくタイムライン上の時間で決まります。`loop` で繰り返されることはなく、`playbackRate` や `pitch` で伸び縮みもしません。クリップより先にファイルが終わる場合、フェードアウトは音が止まったあとにかかります。
- 指定できるのは `Audio` と、プロジェクト JSON の `audio` アイテム（時刻で指定）だけです。`Video` の音声や `Dialogue` の声には使えません。負の値はエラーになります。ブラウザ書き出しでも同じようにかかります。

## 声の下で音楽を下げる

`duckGroup` でクリップをグループに入れ、`duck` を指定したクリップは、そこに挙げたグループのクリップが鳴っているあいだ音量が下がります。`Dialogue` の声は最初から `"voice"` グループに入っているので、台詞の下で BGM を下げるには `duck` だけで足ります。

```tsx
<Audio src="bgm.mp3" volume={0.6} duck={{ under: 'voice' }} />
<Video src="game.mp4" duck={{ under: 'voice', to: { db: -12 }, release: 0.8 }} />
```

- `to` は声の下での音量です。0〜1 の倍率か、0 以下の `{ db }` で指定します。既定値は 0.3 で、`volume` とフェードに掛け合わされます。
- `attack` は下がりきるまでの秒数で、声が始まる時点で下がりきります（既定値 0.15）。`release` は声が終わってから戻るまでの秒数です（既定値 0.4）。0 にすると瞬時に切り替わり、素材によってはクリック音が出ることがあります。
- `holdGap` 秒（既定値 0.6）より短い無音では音量を戻しません。台詞のあいだで音楽が上下しないためです。
- 声は、始まりから、音声ファイルが終わるか `Sequence` が終わるかの早いほうまでを数えます。声のあとも出し続ける字幕で音楽が下がったままになることはありません。ループする声は、シーケンス全体を数えます。ミュートしたクリップは数えません。
- プレビュー、書き出し、ブラウザ書き出しで同じ音量の変化になります。プロジェクト JSON では、`audio` と `video` のアイテムが `duckGroup` と `duck` を、`dialogue` のアイテムが `duckGroup` を取ります。

## ピッチを変える

`Audio` のピッチは既定では速度に連動します。テープと同じで、`playbackRate={1.25}` にすると速く、高くなります。2 つの props で切り離せます。

```tsx
<Audio src="bgm.mp3" playbackRate={1.25} preservePitch pitch={-2} />
```

- `pitch` はクリップを半音単位で移調します。既定値は 0、範囲は −24〜24 で、小数はセント（`0.5` は 50 セント）です。数値またはキーフレームを指定でき、キーフレームは `volume` と同じく `frameKeyframes` で作れます。範囲外の値は、React では ±24 に丸められ、プロジェクト JSON では検証エラーになります。
- `preservePitch` の既定値は `false` で、`playbackRate` は従来どおり速度とピッチを一緒に変えます。`true` にすると、`playbackRate` は再生時間だけを変え、ピッチは `pitch` だけで決まります。
- 聞こえる音は `2^(pitch / 12)` 倍の周波数になり、`preservePitch` を指定しないときはさらに `playbackRate` 倍になります。`pitch={12}` で 1 オクターブ上がります。
- 指定できるのは `Audio` だけです。`Video` の音声や `Dialogue` の声には使えません。プレビューと書き出しは同じ音になりますが、`@celesta/web` のブラウザ書き出しは、これらを使ったクリップを「Audio pitch and preservePitch are not supported by browser export yet.」のエラーで拒否します。

## メディアに合わせてシーンの長さを決める

任意の `prepare()` エクスポートは描画前に一度だけ実行され、`async` にできます。この例は `preloadMedia()` で動画の長さを取得し、`mediaDurationInFrames()` でフレーム数に変換するので、エンドカードが常に映像の直後に来ます。

## 音に合わせて動かす

`loadAudioSpectrum()` は `prepare()` の中で音声ファイル（または動画の音声）をデコードし、全体を解析します。描画中は `useAudioSpectrum()` が現在のフレームの周波数帯と音量を返すので、バーやリング、光を音楽や声に合わせて動かせます。

```tsx file=examples/audio-spectrum.tsx title="audio-spectrum.tsx"
```

- `bands` は低い周波数から高い周波数への各帯の強さで、それぞれ 0〜1 です。帯の強さは FFT のパワーの平均を、フルスケールの正弦波を基準とした dB で表し、`minDecibels`（既定 −80）から `maxDecibels` を 0〜1 に対応させたものです。`maxDecibels` の既定は `'auto'` で、ファイル全体でいちばん大きい帯の強さが上限になるので、どれだけ大きな音量に仕上げたファイルでも 1 に届くのはいちばん大きいところだけです。複数のファイルを同じ尺度で並べたいときなどは、`maxDecibels: -6` のように数値で上限を決められます。`rms` はサンプルの二乗平均平方根で、フルスケールの正弦波ではおよそ 0.707 になります。
- `fftSize` は 1 回の FFT のサンプル数（2 の累乗、既定 2048）です。`bands` は帯の数（既定 32）、`scale` は `minFrequency`（20 Hz）から `maxFrequency`（16 kHz）までの分け方です。`'log'`（既定）は 1 オクターブごとに同じ数の帯、`'linear'` は Hz で同じ幅、`'mel'` は聴感上の音の高さに沿って分けます。
- `smoothing` は秒数、または立ち上がりと立ち下がりを分けた `{ attack, release }` で指定します。その時間がたつと、値は変化のおよそ 63% まで進みます。既定の 0 では平滑化しません。
- ファイルは `hopSeconds`（既定 1/240 秒）ごとに先頭から順に一度だけ解析されます。窓はハン窓で、各時刻を中心に置きます。2 回の解析の間にあるフレームは、その間を補間します。そのため、フレームの値は fps にも描画の順序にもよらず、プレビューと書き出しで同じになります。
- hook は `Audio` と同じく、囲んでいる `Sequence` の時計に従います。`Audio` に `startFrom` があるときは同じ値を `startFrom` に渡してください。`playbackRate` には追従しません。ファイルの前後では、すべての値が 0 です。
- `spectrumAt(spectrum, seconds)` はコンポーネントの外で同じ値を返し、`analyzeAudioSpectrum({ sampleRate, samples }, options)` は手元のサンプルを解析します。
- 複数のチャンネルは平均し、ファイル本来のサンプルレートのまま解析します。相対パスと URL は `preloadMedia()` と同じように解決されます。
