import type { ComponentType } from 'react';

import type { SceneId } from '../../script.ts';

/**
 * 1 つのシーン。各シーンのファイルはこれを 1 つ export し、scenes/index.tsx が
 * 並べて描く。どの部品も、そのシーンの <Sequence> の中で描かれる。
 */
export type SceneDefinition = {
  id: SceneId;
  /** 右上のチャプター表示。無いシーン（冒頭・おわり）は出さない */
  title?: string;
  /** 前のシーンから、帯のワイプで切り替えるか */
  wipeIn?: boolean;
  /** カメラの内側に描くもの（背景や図）。立ち絵と一緒に寄る */
  Stage?: ComponentType;
  /** カメラの外側、画面に固定して描くもの（コードパネルや説明パネル） */
  Overlay?: ComponentType;
  /** 描く前に一度だけ読み込むもの。エントリの prepare() から呼ばれる */
  prepare?: () => Promise<void>;
  /** このシーンが描く和文。和文フォントを使う文字だけに絞って読み込むのに使う */
  strings?: readonly string[];
};
