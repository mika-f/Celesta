import type { ReactNode } from 'react';

const tokenPattern = /(\/\/.*$)|('(?:\\.|[^'\\])*'|`(?:\\.|[^`\\])*`|"(?:\\.|[^"\\])*")|\b(import|from|export|default|function|return|const|let|typeof|true|false|null)\b|(<\/?[A-Z][A-Za-z]*|<\/?>|\/>|(?<![=\s])>|^\s*>)|\b(\d+(?:\.\d+)?)\b/g;
const tokenClasses = ['code-comment', 'code-string', 'code-keyword', 'code-tag', 'code-number'];

/** Lightweight, dependency-free syntax highlighting for a single line of code. */
export function highlight(line: string): ReactNode[] {
  const parts: ReactNode[] = [];
  let last = 0;
  for (const match of line.matchAll(tokenPattern)) {
    const group = match.slice(1).findIndex(Boolean);
    if (match.index > last) parts.push(line.slice(last, match.index));
    parts.push(<span key={match.index} className={tokenClasses[group]}>{match[0]}</span>);
    last = match.index + match[0].length;
  }
  if (last < line.length) parts.push(line.slice(last));
  return parts;
}
