// Check that .github/workflows/actions.lock pins, in each workflow's list,
// every action the workflow uses, counting those of the local composite
// actions it uses (`uses: $/.github/actions/<name>`). GitHub checks a
// workflow's list only when the workflow starts, so a gap in a tag-only
// workflow (plugin-release.yml, package.yml) showed first on a release tag.
// CI runs this on every pull request; it reads every workflow, run or not.
//
//   node scripts/check-actions-lock.mts [repository root]
//
// After changing a workflow's actions, run `gh actions-lock` to update the
// lock, then this.
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

import { LOCK_PATH, checkActionsLock } from './plugins/actions-lock.mts';

const root = resolve(process.argv[2] ?? resolve(dirname(fileURLToPath(import.meta.url)), '..'));

let result;
try {
  result = checkActionsLock(root);
} catch (error) {
  console.error((error as Error).message);
  process.exit(2);
}

const annotate = process.env.GITHUB_ACTIONS === 'true';
for (const { file, message } of result.problems) {
  console.error(annotate ? `::error file=${file}::${message}` : message);
}
if (result.problems.length > 0) {
  console.error(`Run \`gh actions-lock\` to update ${LOCK_PATH}.`);
  process.exit(1);
}
console.log(`${LOCK_PATH} pins every action of the ${result.workflows.length} workflows.`);
