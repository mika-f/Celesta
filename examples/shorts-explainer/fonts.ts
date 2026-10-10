import { CAST } from './character';
import { SCRIPT, plainText } from './script';
import { TEXT as FRAME } from './scenes/FrameScene';
import { TEXT as HOOK } from './scenes/Hook';
import { TEXT as OUTRO } from './scenes/Outro';
import { TEXT as PSD } from './scenes/PsdScene';
import { TEXT as REACT } from './scenes/ReactScene';
import { TEXT as SYNC } from './scenes/SyncScene';
import { TEXT as VOICE } from './scenes/VoiceScene';

// The Japanese faces are subset to exactly the characters this video draws
// (the Google Fonts `text=` parameter), so they load as a few small files.
// Subtitles come from the script; every other Japanese string is listed in
// its scene's TEXT. Mora kana come from the voices, so all kana are included.
const KANA = Array.from({ length: 0x30ff - 0x3041 + 1 }, (_, i) => String.fromCodePoint(0x3041 + i)).join('');
const JA_TEXT = [
  ...SCRIPT.map((line) => plainText(line.text)),
  ...Object.values(CAST).map((c) => c.displayName),
  ...HOOK, ...REACT, ...FRAME, ...VOICE, ...PSD, ...SYNC, ...OUTRO,
  KANA, '▼→＝（）・：　',
  // Printable ASCII, for Latin words set in the Japanese faces.
  Array.from({ length: 0x7e - 0x20 + 1 }, (_, i) => String.fromCodePoint(0x20 + i)).join(''),
].join('');
const GLYPHS = encodeURIComponent([...new Set(JA_TEXT)].sort().join(''));

export const FONTS = [
  'https://fonts.googleapis.com/css2?family=JetBrains+Mono:wght@500;700;800',
  `https://fonts.googleapis.com/css2?family=Noto+Sans+JP:wght@700;800;900&family=Mochiy+Pop+One&text=${GLYPHS}`,
];
