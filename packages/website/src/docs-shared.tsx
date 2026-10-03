import type { ReactNode } from 'react';

export function Note({ title, children }: { title: string; children: ReactNode }) {
  return <aside className="doc-note"><strong>{title}</strong><div>{children}</div></aside>;
}

export function Api({ rows, caption }: { caption: string; rows: [ReactNode, ReactNode][] }) {
  return <div className="doc-table-wrap doc-api" tabIndex={0} role="region" aria-label={caption}><table><caption>{caption}</caption><thead><tr><th>API</th><th>Use it for</th></tr></thead><tbody>
    {rows.map(([name, use], i) => <tr key={i}><td>{name}</td><td>{use}</td></tr>)}
  </tbody></table></div>;
}
