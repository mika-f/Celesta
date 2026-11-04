import { Assets, Font } from '@celesta.studio/react';
import { japaneseText } from '../components/theme';
import { MORE_STRINGS } from './More';
import { PRS } from './prs';
import { DEMO_STRINGS } from './strings';

// Every Japanese string the film draws, for the Noto Sans JP subset.
const JA_STRINGS = [...PRS.map(([, title]) => title), ...MORE_STRINGS, ...DEMO_STRINGS];
/** The characters Noto Sans JP is loaded with: each once, in order of first use. */
export const JA_GLYPHS = [...new Set(japaneseText(JA_STRINGS))].join('');

export function Fonts() {
  // Latin faces by family name; Japanese subset to the film's own glyphs (#387).
  return (
    <Assets>
      <Font google="Bebas Neue" />
      <Font google="Space Grotesk" weights={[400, 500, 700]} />
      <Font google="JetBrains Mono" weights={[400, 700]} />
      <Font google="Noto Sans JP" weights={[500, 700]} text={JA_GLYPHS} />
    </Assets>
  );
}
