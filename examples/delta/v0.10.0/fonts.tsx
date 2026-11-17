import { Assets, Font } from '@celesta.studio/react';
import { japaneseText } from '../components/theme';
import { PRS } from './prs';

// Every Japanese string the film draws, for the Noto Sans JP subset: only
// the PR titles (the open and the end card count them; the cards draw none).
const JA_STRINGS = PRS.map(([, title]) => title);
/** The characters Noto Sans JP is loaded with: each once, in order of first use. */
export const JA_GLYPHS = [...new Set(japaneseText(JA_STRINGS))].join('');

/** The film's fonts. */
export function Fonts() {
  return (
    <Assets>
      <Font google="Bebas Neue" />
      <Font google="Space Grotesk" weights={[400, 500, 700]} />
      <Font google="JetBrains Mono" weights={[400, 700]} />
      {JA_GLYPHS ? <Font google="Noto Sans JP" weights={[500, 700]} text={JA_GLYPHS} /> : null}
    </Assets>
  );
}
