// Check that .github/workflows/actions.lock pins, in each workflow's list,
// every action the workflow uses, counting those of the local composite
// actions it uses (`uses: $/.github/actions/<name>`), and that it lists the
// actions each of those actions uses in turn (actions/attest-build-provenance
// uses actions/attest), read from their action.yml on GitHub at the locked
// commit. GitHub checks a workflow's pins only when the workflow starts, so a
// gap in a tag-only workflow (plugin-release.yml, package.yml) showed first on
// a release tag. CI runs this on every pull request; it reads every workflow,
// run or not.
//
//   node scripts/check-actions-lock.mts [--offline] [repository root]
//
// `--offline` skips the actions' action.yml files, which need the network.
// After changing a workflow's actions, run `gh actions-lock` to update the
// lock, then this.
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

import { LOCK_PATH, checkActionsLock, checkNestedActions } from './plugins/actions-lock.mts';

const args = process.argv.slice(2);
const offline = args[0] === '--offline';
if (offline) args.shift();
const root = resolve(args[0] ?? resolve(dirname(fileURLToPath(import.meta.url)), '..'));

let result;
try {
  const actions = new Set<string>();
  result = checkActionsLock(root, { actions });
  if (!offline) result.problems.push(...(await checkNestedActions(root, actions)));
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
const nested = offline ? '' : ', and the actions they use,';
console.log(`${LOCK_PATH} pins every action${nested} of the ${result.workflows.length} workflows.`);
