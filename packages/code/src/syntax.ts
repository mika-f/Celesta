import { tokenize as bash } from '@twinkleplop/bash';
import { tokenize as json } from '@twinkleplop/json';
import { tokenize as tsx } from '@twinkleplop/tsx';
import { tokenize as ts } from '@twinkleplop/typescript';

export type CodeLanguage = 'tsx' | 'ts' | 'json' | 'bash' | 'text';

export interface CodeToken {
  /** Original source, including whitespace. */
  text: string;
  /** twinkleplop token name; JSON keys are `property`, unclassified text is `plain`. */
  type: string;
  /** Start (inclusive) and end (exclusive), in UTF-16 source offsets. */
  start: number;
  end: number;
}

const tokenizers = new Map([
  ['tsx', tsx()], ['ts', ts()], ['json', json()], ['bash', bash()],
]);

/** Tokenizes the complete source without producing HTML or requiring a DOM. */
export function tokenizeCode(source: string, language: CodeLanguage = 'text'): CodeToken[] {
  if (typeof source !== 'string') throw new Error('Code source must be a string');
  if (language === 'text') return source ? [{ text: source, type: 'plain', start: 0, end: source.length }] : [];
  const tokenize = tokenizers.get(language);
  if (!tokenize) throw new Error(`Unsupported code language: ${language}`);
  const { tokens, token_types } = tokenize(source);
  const result: CodeToken[] = [];
  let offset = 0;
  const append = (type: string, start: number, end: number) => {
    if (end > start) result.push({ text: source.slice(start, end), type, start, end });
  };
  for (let i = 0; i < tokens.length; i += 3) {
    const type = tokens[i];
    const start = tokens[i + 1];
    const end = tokens[i + 2];
    append('plain', offset, start);
    append(token_types[type], start, end);
    offset = end;
  }
  append('plain', offset, source.length);
  if (language === 'json') {
    for (let start = 0; start < result.length; start++) {
      if (result[start].type !== 'string' || !result[start].text.startsWith('"')) continue;
      // Escapes split a JSON string into multiple tokens. Color the entire key,
      // including those fragments, after identifying its closing quote and colon.
      let end = start + 1;
      while (end < result.length && (result[end].type === 'string' || result[end].type === 'string_escape')) end++;
      const last = result[end - 1];
      let next = end;
      while (next < result.length && /^[ \t\r\n]*$/.test(result[next].text)) next++;
      if (last.type === 'string' && last.text.endsWith('"') && last.end > result[start].start + 1
        && result[next]?.text.startsWith(':')) {
        for (let i = start; i < end; i++) result[i].type = 'property';
      }
      start = end - 1;
    }
  }
  return result;
}

export function validateTabSize(tabSize: number): void {
  if (!Number.isSafeInteger(tabSize) || tabSize < 1) throw new Error('Code tabSize must be a positive integer');
}

/** Tab stops count code points, as do Code's columns and useTypewriter(). */
export function expandTabs(text: string, tabSize: number, column = 0): string {
  let result = '';
  for (const character of text) {
    const expanded = character === '\t' ? ' '.repeat(tabSize - column % tabSize) : character;
    result += expanded;
    column += character === '\t' ? expanded.length : 1;
  }
  return result;
}

export interface CodeRun {
  type: string;
  text: string;
  column: number;
}

export interface CodeLine {
  text: string;
  runs: CodeRun[];
  start: number;
  /** Expanded columns at each original source code-point boundary. */
  columns: number[];
}

export function codeLines(tokens: readonly CodeToken[], tabSize: number): CodeLine[] {
  const lines: CodeLine[] = [{ text: '', runs: [], start: 0, columns: [0] }];
  let offset = 0;
  let column = 0;
  let previousCR = false;
  for (const token of tokens) {
    for (const character of token.text) {
      const line = lines[lines.length - 1];
      if (character === '\r' || character === '\n') {
        if (character !== '\n' || !previousCR) lines.push({ text: '', runs: [], start: offset + 1, columns: [0] });
        if (character === '\n' && previousCR) lines[lines.length - 1].start = offset + 1;
        column = 0;
      } else {
        let run = line.runs[line.runs.length - 1];
        if (!run || run.type !== token.type) {
          run = { type: token.type, text: '', column };
          line.runs.push(run);
        }
        const expanded = expandTabs(character, tabSize, column);
        run.text += expanded;
        line.text += expanded;
        column += Array.from(expanded).length;
        line.columns.push(column);
      }
      previousCR = character === '\r';
      offset++;
    }
  }
  return lines;
}
