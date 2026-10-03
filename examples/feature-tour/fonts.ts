import { CHAPTERS } from './chapters';
import { PROMPT } from './chapters/AgentDemo';
import { SPECIMENS } from './chapters/TypeDemo';
import { VOWEL_KANA } from './chapters/VoiceDemo';
import { TAGLINE } from './scenes/Open';
import { VOICE_TEXT } from './voice';

// Japanese faces are subset to exactly the characters this film uses (the
// Google Fonts `text=` parameter), so they load as a few small files. Any new
// Japanese string must be listed here too.
export const JA_TEXT = [
  ...CHAPTERS.flatMap((c) => [c.ja, c.jaShort]),
  ...SPECIMENS.flatMap((s) => [s.glyph, s.sample]),
  ...Object.values(VOWEL_KANA),
  VOICE_TEXT, TAGLINE, PROMPT, '機能一覧',
].join('');
export const JA_GLYPHS = encodeURIComponent([...new Set(JA_TEXT)].sort().join(''));
export const LATIN_FONTS = 'https://fonts.googleapis.com/css2?family=Unbounded:wght@800'
  + '&family=Instrument+Serif&family=JetBrains+Mono:wght@400;500;700';
export const JA_FONTS = 'https://fonts.googleapis.com/css2?family=Noto+Sans+JP:wght@500;700;900'
  + `&family=Dela+Gothic+One&family=DotGothic16&text=${JA_GLYPHS}`;
