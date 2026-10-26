// 台本。film.tsx の prepare() が、この行を VOICEVOX で読み上げ（prepareNarration）、
// 音声の長さから全台詞のフレームを決める（planDialogue）。行を足したり直したり
// すれば、次にプレビューや書き出しをしたときに、変わった行だけが読み直される。
//
// - text:       字幕に出す文。改行したい位置は \n で書く（ほかは文節の切れ目で折り返す）
// - spokenText: VOICEVOX に読ませる文（英単語をカタカナにする）。省略時は text
// - style:      VOICEVOX のスタイル ID
// - face:       立ち絵の表情。ずんだもんは PSD の表情フォルダ、めたんは PSD ファイル
// - gap:        この行のあとに空ける秒数（省略時は src/timing.ts の既定）
//
// 値だけのファイルにしておく（ほかのファイルを import しない）。

export type Speaker = 'zunda' | 'metan';

export type ZundaFace = 'normal' | 'amaama' | 'tsuntsun' | 'aori' | 'namida' | 'herohero';
export type MetanFace = 'talk' | 'happy' | 'worried' | 'what';

export type SceneId = 'void' | 'layers' | 'effects' | 'path' | 'timeline' | 'voice' | 'rewind' | 'agent' | 'outro';

export type Line = {
  id: string;
  scene: SceneId;
  style: number;
  text: string;
  spokenText?: string;
  gap?: number;
} & ({ speaker: 'zunda'; face: ZundaFace } | { speaker: 'metan'; face: MetanFace });

// VOICEVOX のスタイル ID
const Z = { normal: 3, amaama: 1, tsuntsun: 7, herohero: 75, namida: 76 };
const M = { normal: 2, amaama: 0, tsuntsun: 6 };

export const LINES: Line[] = [
  // 0 · 真っ暗なコンポジション
  { id: 'v1', scene: 'void', speaker: 'zunda', face: 'normal', style: Z.normal,
    text: '……真っ暗なのだ。', gap: 0.4 },
  { id: 'v2', scene: 'void', speaker: 'metan', face: 'talk', style: M.normal,
    text: 'まだ何も書いてないもの。\nここから一緒に作っていきましょう。', gap: 0.5 },

  // 1 · レイヤーを重ねる
  { id: 'l1', scene: 'layers', speaker: 'zunda', face: 'amaama', style: Z.amaama,
    text: 'ボクはずんだもん。今日はCelestaを紹介するのだ！',
    spokenText: 'ボクはずんだもん。今日はセレスタを紹介するのだ！' },
  { id: 'l2', scene: 'layers', speaker: 'metan', face: 'happy', style: M.normal,
    text: '四国めたんよ。Celestaは、動画をReactのコードで書くツールなの。',
    spokenText: '四国めたんよ。セレスタは、動画をリアクトのコードで書くツールなの。' },
  { id: 'l3', scene: 'layers', speaker: 'zunda', face: 'normal', style: Z.normal,
    text: 'まずは背景がほしいのだ。' },
  { id: 'l4', scene: 'layers', speaker: 'metan', face: 'talk', style: M.normal,
    text: 'Rectを一枚置けば背景になるわ。\nグラデーションも指定できるの。',
    spokenText: 'レクトを一枚置けば背景になるわ。グラデーションも指定できるの。' },
  { id: 'l5', scene: 'layers', speaker: 'zunda', face: 'normal', style: Z.normal,
    text: '写真も貼れるのだ？' },
  { id: 'l6', scene: 'layers', speaker: 'metan', face: 'talk', style: M.normal,
    text: 'Imageのfitをcoverにすれば、画面いっぱいに敷けるわ。',
    spokenText: 'イメージのフィットをカバーにすれば、画面いっぱいに敷けるわ。', gap: 0.4 },

  // 2 · エフェクト
  { id: 'e1', scene: 'effects', speaker: 'zunda', face: 'amaama', style: Z.amaama,
    text: 'タイトルを出したいのだ！ピカピカ光らせたいのだ！' },
  { id: 'e2', scene: 'effects', speaker: 'metan', face: 'talk', style: M.normal,
    text: 'glowで光、shadowで影、blurでぼかし。\n数値はフレームごとに変えられるわ。',
    spokenText: 'グロウで光、シャドウで影、ブラーでぼかし。数値はフレームごとに変えられるわ。', gap: 0.4 },

  // 3 · Path で描く
  { id: 'p1', scene: 'path', speaker: 'zunda', face: 'normal', style: Z.normal,
    text: 'ずんだ餅を描いてほしいのだ。' },
  { id: 'p2', scene: 'path', speaker: 'metan', face: 'talk', style: M.normal,
    text: '線はPathで描くの。\nベジェ曲線も、一本の線として扱えるわ。',
    spokenText: '線はパスで描くの。ベジェ曲線も、一本の線として扱えるわ。' },
  { id: 'p3', scene: 'path', speaker: 'zunda', face: 'amaama', style: Z.amaama,
    text: 'おいしそうなのだ……。', gap: 0.4 },

  // 4 · 動画素材とタイムライン
  { id: 't1', scene: 'timeline', speaker: 'metan', face: 'talk', style: M.normal,
    text: '撮った動画を並べたいときは、JSONのタイムラインも使えるわ。',
    spokenText: '撮った動画を並べたいときは、ジェイソンのタイムラインも使えるわ。' },
  { id: 't2', scene: 'timeline', speaker: 'zunda', face: 'amaama', style: Z.amaama,
    text: 'クリップをトラックに置くだけで、そのまま再生されるのだ！', gap: 0.4 },

  // 5 · しゃべる・口パク
  { id: 's1', scene: 'voice', speaker: 'metan', face: 'happy', style: M.amaama,
    text: 'ところで、ずんだもんの口、ちゃんと動いてるの気づいてた？' },
  { id: 's2', scene: 'voice', speaker: 'zunda', face: 'normal', style: Z.normal,
    text: 'えっ。ボクの口、勝手に動いてるのだ！' },
  { id: 's3', scene: 'voice', speaker: 'metan', face: 'talk', style: M.normal,
    text: '音声と読みがなから、口の形を自動で割り当ててるの。\n表情もPSDのレイヤーで切り替えられるわ。',
    spokenText: '音声と読みがなから、口の形を自動で割り当ててるの。表情もピーエスディーのレイヤーで切り替えられるわ。' },
  { id: 's4', scene: 'voice', speaker: 'zunda', face: 'tsuntsun', style: Z.tsuntsun,
    text: 'ボクの顔で遊ばないでほしいのだ！', gap: 0.4 },

  // 6 · 巻き戻し
  { id: 'r1', scene: 'rewind', speaker: 'metan', face: 'talk', style: M.normal,
    text: 'Celestaの絵は、全部フレーム番号から計算されるの。だから、どこからでも同じ絵になる。',
    spokenText: 'セレスタの絵は、全部フレーム番号から計算されるの。だから、どこからでも同じ絵になる。' },
  { id: 'r2', scene: 'rewind', speaker: 'zunda', face: 'amaama', style: Z.amaama,
    text: 'つまり、さっきのシーンもいつでも呼び出せるのだ！', gap: 0.4 },

  // 7 · エージェント
  { id: 'a1', scene: 'agent', speaker: 'zunda', face: 'herohero', style: Z.herohero,
    text: 'でも、コードを書くのは大変そうなのだ……。' },
  { id: 'a2', scene: 'agent', speaker: 'metan', face: 'happy', style: M.normal,
    text: 'そこでAIの出番。CelestaにはAIエージェント向けのスキルがあるの。',
    spokenText: 'そこでエーアイの出番。セレスタにはエーアイエージェント向けのスキルがあるの。' },
  { id: 'a3', scene: 'agent', speaker: 'zunda', face: 'normal', style: Z.normal,
    text: '……もしかして、この動画も？' },
  { id: 'a4', scene: 'agent', speaker: 'metan', face: 'happy', style: M.normal,
    text: 'そう。台本も、音声の生成も、映像も、ぜんぶClaudeが書いたコードなの。',
    spokenText: 'そう。台本も、音声の生成も、映像も、ぜんぶクロードが書いたコードなの。' },
  { id: 'a5', scene: 'agent', speaker: 'zunda', face: 'namida', style: Z.namida,
    text: 'ボクの出番が奪われたのだ！', gap: 0.5 },

  // 8 · おわり
  { id: 'o1', scene: 'outro', speaker: 'metan', face: 'happy', style: M.normal,
    text: '動画は、コードで書ける。', gap: 0.1 },
  { id: 'o2', scene: 'outro', speaker: 'zunda', face: 'amaama', style: Z.amaama,
    text: 'Celesta、よろしくなのだ！', spokenText: 'セレスタ、よろしくなのだ！', gap: 1.2 },
];
