// Type-checks the playground scene and the documentation examples against
// the declarations `packages/cli`'s build stages (dist/project-types), as an
// entry of a project sees them: copied to `.celesta/` (which ignores itself
// in Git), with the first-party component plugins the examples import as
// `@celesta-plugin/<id>` in `.celesta/plugins/<id>/`, where Celesta puts an
// installed plugin's declarations. Those plugins are made from the packages
// the runtime still serves, so their staged declarations are the plugins'.
//
//   node scripts/check-examples.mjs   (pnpm check:examples)
import { spawnSync } from 'node:child_process';
import { cpSync, existsSync, rmSync, statSync, writeFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const website = fileURLToPath(new URL('../', import.meta.url));
const template = join(website, '..', 'cli', 'dist', 'project-types');
if (!existsSync(join(template, 'version.json'))) {
  console.error(`Missing ${template}. Run \`pnpm run build:runtime\` at the repository root first.`);
  process.exit(1);
}
const PLUGINS = ['aivis-speech', 'code', 'lottie', 'voicevox'];

const support = join(website, '.celesta');
rmSync(support, { recursive: true, force: true });
cpSync(template, support, { recursive: true });
writeFileSync(join(support, '.gitignore'), '# Staged by scripts/check-examples.mjs.\n*\n');
for (const id of PLUGINS) {
  cpSync(join(support, 'node_modules', '@celesta.studio', id, 'dist'), join(support, 'plugins', id), {
    recursive: true,
    filter: (source) => statSync(source).isDirectory() || source.endsWith('.d.ts'),
  });
}

const tsc = join(
  dirname(createRequire(join(website, '..', 'cli', 'package.json')).resolve('typescript/package.json')),
  'bin',
  'tsc',
);
const { status } = spawnSync(process.execPath, [tsc, '-p', join(website, 'tsconfig.examples.json')], { stdio: 'inherit' });
process.exit(status ?? 1);
