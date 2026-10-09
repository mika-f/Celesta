import { t, text } from './i18n';
import type { ReactNode } from 'react';

export function Note({ title, children }: { title: string; children: ReactNode }) {
  return <aside className="doc-note"><strong>{title}</strong><div>{children}</div></aside>;
}

/** API rows, optionally with the package each one is imported from. */
export function Api({ rows, caption }: { caption: string; rows: ([ReactNode, ReactNode] | [ReactNode, ReactNode, string])[] }) {
  const packages = rows.some(row => row.length > 2);
  return <div className="doc-table-wrap doc-api" tabIndex={0} role="region" aria-label={caption}><table><caption>{caption}</caption><thead><tr><th>{text('docs.shared.api.api')}</th>{packages && <th>{text('docs.shared.api.package')}</th>}<th>{text('docs.shared.api.use-it-for')}</th></tr></thead><tbody>
    {rows.map(([name, use, from], i) => <tr key={i}><td>{name}</td>{packages && <td className="doc-api-package">{from?.split(', ').map(name => <code key={name}>{name}</code>)}</td>}<td>{use}</td></tr>)}
  </tbody></table></div>;
}
