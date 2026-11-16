`@celesta-plugin/lottie` は、After Effects から Bodymovin で書き出した Lottie の JSON を、コンポジションのフレームに合わせて描きます。各フレームはその時刻だけから決まるパス・グループ・マスクとして描かれるので、プレビュー、書き出し、`@celesta.studio/web` のどれでも同じ絵になり、どのフレームへスクラブしても再生したときと同じ結果になります。

> [!NOTE]
> **プラグインを追加する**
> `@celesta-plugin/lottie` は Celesta の[プラグインレジストリ](/ja/docs/plugins/#レジストリのプラグイン)にある公式プラグインです。`celesta` コマンドでプロジェクトに追加すると、`celesta.config.json` に宣言し、`celesta.lock` にバージョンを記録して、ダウンロードします。
>
> ```sh
> celesta plugin add lottie
> ```
>
> プラグインのバージョンは Celesta と同じです。Celesta のプレリリースでは、`celesta plugin add lottie@0.10.0-alpha.4` のようにそのバージョンを指定してください。プレリリースを含まない指定はプレリリースに一致しません。デスクトップアプリはまだ `celesta.lock` を読まないため、アプリのプレビューと書き出しには `celesta plugin install --global lottie` でも全プロジェクト向けにインストールしてください（プレリリースでは同じくバージョンを指定します）。
>
> `@celesta.studio/lottie` をインポートするコンポジションもそのまま動きます。Celesta はそのパッケージを引き続き同梱し、使うべきインポートと実行する `celesta plugin add` コマンドを警告で示します。

ブラウザのプレイグラウンド（`@celesta.studio/web`）ではプラグインは不要です。`@celesta-plugin/lottie` をそのまま使えます。

## アニメーションを置く

JSON ファイルを読み込み、その内容を `data` に渡します。

```tsx
import { Composition, Sequence } from '@celesta.studio/react';
import { Lottie } from '@celesta-plugin/lottie';

import confetti from './confetti.json';

export default function Root() {
  return (
    <Composition width={1920} height={1080} fps={60} durationInFrames={300}>
      <Lottie data={confetti} x={960} y={540} anchorX={0.5} anchorY={0.5} width={800} />
      <Sequence from={120}>
        <Lottie data={confetti} endFrame={30} playbackRate={2} loop={false} />
      </Sequence>
    </Composition>
  );
}
```

| Props | 用途 |
| --- | --- |
| `data` | Lottie の JSON の内容です。 |
| `width` / `height` | 描く大きさです。既定はファイルの `w` × `h` で、片方だけなら縦横比を保ちます。この枠の外は描かれません。 |
| `playbackRate` | 再生速度です。既定は 1 で、負の値で逆再生します。 |
| `loop` | 既定は true です。false では `endFrame` の 1 フレーム前で止まります。 |
| `startFrame` / `endFrame` | 再生する範囲を、ファイル自身のフレームで指定します。既定はファイルの `ip` / `op` です。 |
| `assetsPath` | 画像レイヤーのファイルがあるフォルダです。`<Image>` の `src` と同じくエントリからの相対パスです。既定は各画像アセットの `u` です。 |
| `onWarning` | 描けない機能を見つけたときのメッセージを受け取ります。既定は `console.warn` で、ファイルごとに 1 回だけ呼ばれます。 |

`x`、`y`、`anchorX`、`anchorY`、`rotation`、`opacity` などの共通の props も使えます。再生は、囲んでいる Sequence のフレーム 0 から始まります。ファイルのフレームの間は補間するので、30 fps のファイルも 60 fps の動画で滑らかに動きます。

## 対応している機能

- レイヤー：シェイプ、プリコンポジション（タイムリマップと伸縮を含む）、平面、画像、ヌル。親子関係、ブレンドモード。
- マスク：加算、減算、交差、反転、不透明度。トラックマット：アルファ、ルミナンス、それぞれの反転。
- シェイプ：グループ、パス、長方形（角丸を含む）、楕円、スター、多角形、塗り、線（破線、線端、角）、線形・円形グラデーション、パスのトリミング。

次の機能は描かれず、警告が出ます：テキストレイヤー、エクスプレッション、エフェクトとレイヤースタイル、3D レイヤー、自動方向、パスの結合、リピーター、角を丸くする、偶奇の塗りルール、円形グラデーションのハイライト、マスクの拡張、埋め込まれた（base64 の）画像。画像は別ファイルとして書き出してください。`inspectLottie(data)` は、描かずに同じメッセージの配列を返します。
