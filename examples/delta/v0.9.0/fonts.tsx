import { Assets, Font } from '@celesta/react';
import { japaneseText } from '../components/theme';
import { Shizuku } from './demos/DialogueDemo';
import { PRS } from './prs';
import { DEMO_STRINGS } from './strings';

// Every Japanese string the film draws, for the Noto Sans JP subset.
const JA_STRINGS = [...PRS.map(([, title]) => title), ...DEMO_STRINGS];
/** The characters Noto Sans JP is loaded with: each once, in order of first use. */
export const JA_GLYPHS = [...new Set(japaneseText(JA_STRINGS))].join('');

/** The film's fonts, and the character DIALOGUE speaks through. */
export function Fonts() {
  return (
    <Assets>
      <Font google="Bebas Neue" />
      <Font google="Space Grotesk" weights={[400, 500, 700]} />
      <Font google="JetBrains Mono" weights={[400, 700]} />
      <Font google="Noto Sans JP" weights={[500, 700]} text={JA_GLYPHS} />
      <Shizuku />
    </Assets>
  );
}
