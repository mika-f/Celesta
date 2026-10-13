import { C } from '../constants.ts';

// The Live edit scene's script: two edits to a title component, each saved
// a little later. The editor shows what has been typed; the preview shows
// only what has been saved. make-score.py clicks on SAVE_FRAMES.

export type TitleProps = { fontSize: number; fill: string };

const INITIAL: TitleProps = { fontSize: 96, fill: C.fg };

type Edit = { key: keyof TitleProps; at: number; end: number; save: number; from: string; to: string };

export const EDITS: Edit[] = [
  { key: 'fontSize', at: 30, end: 60, save: 75, from: '96', to: '160' },
  { key: 'fill', at: 120, end: 150, save: 165, from: C.fg, to: C.accent },
];

export const SAVE_FRAMES = EDITS.map((e) => e.save);

// The text of an edited value at a local frame: the old value is deleted
// over the first third of the edit, the new one typed over the rest.
function typedValue(edit: Edit, local: number): string {
  if (local < edit.at) return edit.from;
  if (local >= edit.end) return edit.to;
  const third = (edit.end - edit.at) / 3;
  if (local < edit.at + third) {
    const removed = Math.floor(((local - edit.at) / third) * edit.from.length) + 1;
    return edit.from.slice(0, Math.max(0, edit.from.length - removed));
  }
  const typed = Math.floor(((local - edit.at - third) / (third * 2)) * edit.to.length) + 1;
  return edit.to.slice(0, typed);
}

const PREFIX = { fontSize: '        fontSize: ', fill: "        fill: '" } as const;
const LINE = { fontSize: 5, fill: 6 } as const;

export function typedAt(local: number): { source: string; caret: { line: number; column: number } } {
  const size = typedValue(EDITS[0], local);
  const fill = typedValue(EDITS[1], local);
  const source = `export default function Title() {
  return (
    <Text x={960} y={540} anchorX={0.5}
      style={{
${PREFIX.fontSize}${size},
${PREFIX.fill}${fill}',
      }}>
      Hello, Celesta.
    </Text>
  );
}`;
  const active = local >= EDITS[1].at ? EDITS[1] : EDITS[0];
  const value = active === EDITS[0] ? size : fill;
  return { source, caret: { line: LINE[active.key], column: PREFIX[active.key].length + value.length + 1 } };
}

export function savedAt(local: number): TitleProps {
  const props = { ...INITIAL };
  if (local >= EDITS[0].save) props.fontSize = Number(EDITS[0].to);
  if (local >= EDITS[1].save) props.fill = EDITS[1].to;
  return props;
}
