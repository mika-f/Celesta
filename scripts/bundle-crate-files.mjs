// Copy the repository files that crates embed into each crate's `bundled/`,
// since a crate published to crates.io cannot reach outside its own folder.
// Run it before `cargo package` or `cargo publish`; the copies are not committed.
import { cpSync, rmSync } from 'node:fs';
import { fileURLToPath } from 'node:url';

const root = fileURLToPath(new URL('../', import.meta.url));
const bundles = [
  ['skills/celesta/SKILL.md', 'crates/mcp/bundled/skills/celesta/SKILL.md'],
  ['skills/celesta/references', 'crates/mcp/bundled/skills/celesta/references'],
  [
    'examples/editor-demo.celesta.json',
    'crates/editor/bundled/editor-demo.celesta.json',
  ],
];

for (const crate of ['crates/mcp/bundled', 'crates/editor/bundled']) {
  rmSync(root + crate, { recursive: true, force: true });
}
for (const [source, destination] of bundles) {
  cpSync(root + source, root + destination, { recursive: true });
}
