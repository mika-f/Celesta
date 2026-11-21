`@celesta-plugin/score` は、動画の効果音とシンプルな音楽をデータで書くプラグインです。tick・pop・whoosh などの効果音、いくつかの旋律用の音色、ドラム、拍のグリッドを使えます。スコアは映像と同じフレーム・秒・拍に対して記録され、`prepare()` で WAV ファイルに書き出されて `<Audio>` で鳴ります。そのためプレビューと書き出しは同じファイルを鳴らし、そのファイルは何度実行しても、どのシステムでもバイト単位で同じです。音声ファイルは要りません。

> [!NOTE]
> **プラグインを追加する**
> `@celesta-plugin/score` は Celesta の[プラグインレジストリ](/ja/docs/plugins/#レジストリのプラグイン)にある公式プラグインです。`celesta` コマンドでプロジェクトに追加すると、`celesta.config.json` に宣言し、`celesta.lock` にバージョンを記録して、ダウンロードします。
>
> ```sh
> celesta plugin add score
> ```
>
> プラグインのバージョンは Celesta と同じです。Celesta のプレリリースでは、`celesta plugin add score@0.10.0-alpha.4` のようにそのバージョンを指定してください。リリースに含まれるまでは、チェックアウトで `node scripts/plugins/build.mts plugins/score` を実行してビルドし、`Celesta-export` に `--plugin-dir plugins/dist` を渡すか、そのフォルダーを `CELESTA_PLUGIN_PATH` に入れてください。

スコアは Node のファイルシステムで書き出すので、鳴るのはデスクトップアプリと `Celesta-export` です。ブラウザのプレイグラウンド（`@celesta.studio/web`）では鳴りません。

## スコアを書く

`defineScore()` はモジュールの読み込み時にビルダーを 1 回だけ実行し、記録するたびに名前と値を確かめます。`prepare()` で書き出し、`<ScoreAudio>` をコンポジションの最上位に置きます。

```tsx
import { Composition, defineScenes } from '@celesta.studio/react';
import { typewriterFrames } from '@celesta.studio/text';
import { ScoreAudio, defineScore } from '@celesta-plugin/score';

const scenes = defineScenes([
  { id: 'intro', durationInFrames: 120 },
  { id: 'main', durationInFrames: 240 },
]);
const grid = { bpm: 120 }; // useBeat(grid) にも渡せる
const typing = { from: 12, framesPerChar: 3 };

export const score = defineScore({ fps: 30, durationInFrames: scenes.durationInFrames }, s => {
  s.tempo(grid);
  s.master({ gain: 0.8, fadeOut: 1.5 });
  s.ticks(typewriterFrames('npm run build', typing)); // 1 文字に 1 tick
  s.fx('whoosh', { frame: scenes.scenes[1].from, reverse: true }); // カットに向かって膨らむ
  s.program('lead', 'marimba', { pan: -0.2, bright: 0.7 });
  s.run('lead', ['C5', 'D5', 'E5', 'G5'], { beat: 8, step: 0.5, cresc: 0.2 });
  s.drums({ kick: 'x...x...', hat: '.x.x.x.X' }, { beat: 8, repeat: 4 });
});

export async function prepare() {
  await score.prepare();
}

export default function Film() {
  return (
    <Composition width={1920} height={1080} fps={30} durationInFrames={scenes.durationInFrames}>
      {/* … */}
      <ScoreAudio score={score} />
    </Composition>
  );
}
```

リポジトリの `examples/score` は、この方法で作った 12 秒の作品です。

## タイミング

どの呼び出しも、時刻を次のどれか 1 つで受け取ります。

| キー | 意味 |
| --- | --- |
| 数値、または `frame` | コンポジションのフレーム。小数も使えます（128 BPM の拍はフレームの間に来ます）。 |
| `t` | 先頭からの秒。 |
| `beat` | `s.tempo()` で決めたグリッド上の拍。0 から数えます。 |

長さは `frames`・`dur`（秒）・`beats` のどれか 1 つまでです。長さのない音符は、`beat` で始まるときは 1 拍、それ以外は 0.5 秒鳴ります。

`s.tempo({ bpm, beatsPerBar, offset })` は `useBeat()`・`beatAt()` と同じオプションを受け取るので、同じオブジェクトを両方に渡せば映像と音が 1 つのグリッドに乗ります。`offset` は拍 0 が来るフレームです。もう一度呼ぶと、それ以降の呼び出しのグリッドが変わります。`s.beatFrame(n)` は拍が来るフレームを返します。`defineScenes()` や `computeSeries()` のシーン開始、`useCue()` のリストのフレーム、`typewriterFrames()` はどれもフレームなので、そのまま渡せます。

## 呼び出し

| 呼び出し | 記録するもの |
| --- | --- |
| `s.program(track, patch, { gain, pan, reverb, bright })` | トラックに音色を割り当てます。音符は記録した時点のトラックの設定を使います。 |
| `s.note(track, pitch, { …時刻, …長さ, vel })` | 音符 1 つ。音高は MIDI 番号（60 が C4）か、`'C4'`・`'F#3'`・`'Bb2'` のような音名です。 |
| `s.chord(track, pitches, { …時刻, …長さ, vel })` | 同時に鳴る音符。 |
| `s.run(track, pitches, { …時刻, step, cresc, vel })` | 等間隔の音符。`step` の単位は開始と同じで、`frame` ならフレーム、`t` なら秒、`beat` なら拍です。`cresc` は最後の音符までに `vel` へ足されます。 |
| `s.pattern(track, 'x.X.o.xx', { …時刻, step, repeat, pitch, beats, vel })` | 1 文字が `step` 拍（既定は半拍）です。`x` は音符、`X` はアクセント、`o` はゴースト、`.` と `-` は休符で、スペースと `\|` は無視します。`pitch` をリストにすると順に使います。 |
| `s.drums({ kick: 'x...', hat: '.x.x' }, { …時刻, step, repeat, vel })` | 同じ文字で書くドラムの行。行ごとに繰り返します。 |
| `s.drum(name, 時刻または { …時刻, vel, pitch, pan, gain, reverb })` | ドラムを 1 打。 |
| `s.fx(name, 時刻または { …時刻, dur, vel, pitch, pan, gain, reverb, reverse })` | 効果音 1 つ。`pitch` は半音単位で動かします。`dur` は `sparkle`・`whoosh`・`revswell`・`roll` の全体の長さを決め（既定は 0.6・0.6・1.2・1 秒）、音はその中で終わります。 |
| `s.ticks(times, { sound, vary, vel, pitch, pan, gain, reverb })` | 時刻ごとに効果音（既定は `tick`）を 1 つ。`vary` を渡すと、tick ごとの音高がその半音数まで揺れます。揺れ方は毎回同じです。 |
| `s.master({ gain, fadeIn, fadeOut, reverb })` | ミックス全体の音量、秒単位のフェード、残響の量。 |

`vel` は 0〜1 で、既定は 0.8 です。`pan` は -1（左）〜1（右）、`reverb` は残響に送る割合で 0〜1 です。

| 種類 | 名前 |
| --- | --- |
| 音色 | `marimba` `vibes` `glock` `ep` `keys` `pluck` `bass` `pad` |
| ドラム | `kick` `snare` `clap` `sidestick` `hat` `openhat` `shaker` `tamb` `woodblock` `cowbell` `tom` `crash` `ride` |
| 効果音 | `tick` `click` `doubleclick` `pop` `ding` `chime` `sparkle` `whoosh` `revswell` `roll` `boing` `thud` |

音はその時刻から鳴り始めます。ただし `reverse: true` の `whoosh` と `revswell` は、その時刻に向かって膨らんでそこで止まるので、カットやヒットに合わせられます。これらの始まりは 0 より前になってもかまいません。0 より前の部分は鳴りません。

## 間違えたとき

- 知らない音色・ドラム・効果音の名前、パターンに使えない文字、`s.tempo()` より前の `beat`、`s.program()` していないトラックの音符、範囲外の値はエラーになります。メッセージには呼び出しと、綴りを間違えた名前なら近い名前が出ます（`s.fx('wosh'): unknown effect "wosh"; did you mean 'whoosh'?`）。
- 0 より前のイベントはエラーになります。
- コンポジションの終わりと同時か後に鳴り始める音は捨てられ、呼び出しごとに 1 つの警告が `score.warnings` と標準エラーに出ます。`repeat` が終わりを越えたときなどです。逆向きの `whoosh` と `revswell` は指定した時刻の `dur` 前から鳴り始めるので、終わりちょうど、または少し後に合わせても、終わりより前の部分は鳴ります。`defineScore()` に `strict: true` を渡すとエラーにします。終わりの時点でまだ鳴っている音はそこで切れます。

## ファイル

- `prepare()` は、コンポジションとちょうど同じ長さの 48 kHz・16 bit・ステレオの WAV を、エントリーの隣の `.celesta/score/<sha256>.wav` に書きます（変えるときは `prepare({ cacheDir })`）。名前は解決済みのイベント、マスターの設定、シード、シンセサイザーのバージョンのハッシュなので、変わらないスコアは書き直さず、変わったスコアが古いファイルを鳴らすこともありません。
- シンセサイザーは IEEE 754 で丸めが一つに決まる演算とシード付きのノイズだけを使うので、バイト列はシステムにも Node.js のバージョンにもよりません。`defineScore(…, { seed })` でノイズが変わります。
- `score.events` は、すべての音を秒とフレームに解決して並べたものです。音に合わせて何かを描くときに使えます。`score.toWav()` はファイルに書かずにバイト列を返します。
- `<ScoreAudio>` は `volume`・`duckGroup`・`duck` など、`<Audio>` のほかの props をすべて受け取ります。
