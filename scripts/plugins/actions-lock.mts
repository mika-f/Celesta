// The actions each workflow under .github/workflows uses, and the pins
// .github/workflows/actions.lock gives it, for scripts/check-actions-lock.mts.
//
// GitHub refuses to start a workflow whose actions, including those its local
// composite actions use, are not all in that workflow's list in the lock
// ("lockfile missing pin for ..."), and it says so only when the workflow
// runs: for plugin-release.yml and package.yml, on a release tag.
//
// GitHub also refuses one whose actions use an action (a composite action's
// step) that the lock does not list under that action's `uses:`, with its own
// dependencies entry: actions/attest-build-provenance@v4.2.2 uses
// actions/attest at a commit. `checkNestedActions` reads each action's
// action.yml at its locked commit for those.
//
// A workflow and a composite action are read line by line for their `uses:`
// keys (a step's or a job's), which is all the lock covers.

import { existsSync, readFileSync, readdirSync } from 'node:fs';
import { join } from 'node:path';

export const LOCK_PATH = '.github/workflows/actions.lock';
const WORKFLOWS = '.github/workflows';

/** A `uses:` value on its own line, quoted or not, before an optional comment. */
const USES = /^\s*(?:-\s+)?uses:\s*(?:'([^']*)'|"([^"]*)"|([^\s#]+))\s*(?:#.*)?$/;

export type Uses =
  /** A repository's action or reusable workflow, as the lock names it: `owner/repo@ref`. */
  | { kind: 'action'; pin: string }
  /** A composite action in this repository (`./.github/actions/x` or `$/.github/actions/x`). */
  | { kind: 'local-action'; path: string }
  /** A reusable workflow in this repository, which has a list of its own in the lock. */
  | { kind: 'local-workflow'; path: string }
  /** A Docker image (`docker://...`), which the lock does not pin. */
  | { kind: 'docker' };

/** Every `uses:` value in a workflow or an action.yml, in order. */
export function usesValues(text: string): string[] {
  const values: string[] = [];
  for (const line of text.split(/\r?\n/)) {
    const match = USES.exec(line);
    if (match) values.push(match[1] ?? match[2] ?? match[3]);
  }
  return values;
}

/**
 * What a `uses:` value refers to. An action in a repository's subfolder
 * (`actions/cache/restore@v6.1.0`) is pinned by its repository
 * (`actions/cache@v6.1.0`). GitHub compares owners and repositories without
 * case, so `Swatinem/rust-cache` is the lock's `swatinem/rust-cache`.
 */
export function classifyUses(value: string): Uses {
  if (value.startsWith('docker://')) return { kind: 'docker' };
  const local = /^(?:\.|\$)\/(.+?)\/?$/.exec(value);
  if (local) {
    const path = local[1];
    return /\.ya?ml$/.test(path) ? { kind: 'local-workflow', path } : { kind: 'local-action', path };
  }
  const remote = /^([^/@\s]+)\/([^/@\s]+)(?:\/[^@\s]*)?@(\S+)$/.exec(value);
  if (!remote) throw new Error(`cannot read the action in \`uses: ${value}\``);
  const [, owner, repo, ref] = remote;
  return { kind: 'action', pin: `${owner.toLowerCase()}/${repo.toLowerCase()}@${ref}` };
}

export interface ActionsLock {
  /** Each workflow's path and its list of pins (`owner/repo@ref`), lowercased before the `@`. */
  workflows: Map<string, Set<string>>;
  /** The pins under `dependencies:`. */
  dependencies: Set<string>;
  /** Each dependency's commit, without the `sha1-` prefix. */
  commits: Map<string, string>;
  /** The pins each dependency lists under `uses:`: the actions its action.yml uses. */
  uses: Map<string, Set<string>>;
}

const lowerPin = (pin: string) => {
  const at = pin.indexOf('@');
  return at < 0 ? pin.toLowerCase() : pin.slice(0, at).toLowerCase() + pin.slice(at);
};

const unquote = (text: string) => text.replace(/^'(.*)'$|^"(.*)"$/, '$1$2');

/**
 * The `workflows:` and `dependencies:` maps of an actions.lock, as
 * `gh actions-lock` writes it: four spaces before a workflow or a
 * dependency, a list item per pin below a workflow, and eight spaces before
 * a dependency's fields, its `uses:` list among them.
 */
export function parseActionsLock(text: string): ActionsLock {
  const workflows = new Map<string, Set<string>>();
  const dependencies = new Set<string>();
  const commits = new Map<string, string>();
  const uses = new Map<string, Set<string>>();
  let section: string | undefined;
  let workflow: Set<string> | undefined;
  let dependency: string | undefined;
  let nested: Set<string> | undefined;

  for (const raw of text.split(/\r?\n/)) {
    const line = raw.replace(/\s+#.*$/, '');
    if (line.trim() === '' || line.trimStart().startsWith('#')) continue;

    const top = /^(\S[^:]*):/.exec(line);
    if (top) {
      section = top[1];
      workflow = dependency = nested = undefined;
      continue;
    }
    const key = /^ {4}(\S.*?):\s*$/.exec(line);
    if (key) {
      const name = unquote(key[1]);
      if (section === 'workflows') {
        workflow = new Set();
        workflows.set(name, workflow);
      } else if (section === 'dependencies') {
        dependency = lowerPin(name);
        nested = undefined;
        dependencies.add(dependency);
      }
      continue;
    }
    const field = /^ {8}(\w+):\s*(.*?)\s*$/.exec(line);
    if (field && section === 'dependencies' && dependency) {
      const [, name, value] = field;
      nested = undefined;
      if (name === 'commit') commits.set(dependency, unquote(value).replace(/^sha1-/, ''));
      if (name === 'uses') {
        nested = new Set();
        uses.set(dependency, nested);
      }
      continue;
    }
    const item = /^\s+-\s+(\S.*?)\s*$/.exec(line);
    if (!item) continue;
    if (section === 'workflows' && workflow) workflow.add(lowerPin(unquote(item[1])));
    if (section === 'dependencies' && nested) nested.add(lowerPin(unquote(item[1])));
  }
  return { workflows, dependencies, commits, uses };
}

export interface Problem {
  /** The workflow whose list lacks the pin, or the lock for a pin without a commit. */
  file: string;
  message: string;
}

export interface CheckResult {
  workflows: string[];
  problems: Problem[];
}

/**
 * The pins a workflow needs: its own actions and those of the local
 * composite actions it uses, each with the files that use it. A local
 * reusable workflow has its own list in the lock, so it is not followed.
 * `actions` collects the `uses:` values of those pins, subfolders kept.
 */
export function workflowPins(root: string, workflow: string, actions?: Set<string>): Map<string, Set<string>> {
  const pins = new Map<string, Set<string>>();
  const visited = new Set<string>();

  const visit = (file: string) => {
    if (visited.has(file)) return;
    visited.add(file);
    for (const value of usesValues(readFileSync(join(root, file), 'utf8'))) {
      let uses: Uses;
      try {
        uses = classifyUses(value);
      } catch (error) {
        throw new Error(`${file}: ${(error as Error).message}`);
      }
      if (uses.kind === 'action') {
        const from = pins.get(uses.pin) ?? new Set<string>();
        from.add(file);
        pins.set(uses.pin, from);
        actions?.add(value);
      } else if (uses.kind === 'local-action') {
        const action = ['action.yml', 'action.yaml']
          .map((name) => `${uses.path}/${name}`)
          .find((path) => existsSync(join(root, path)));
        if (action === undefined) throw new Error(`${file}: \`uses: ${value}\` has no ${uses.path}/action.yml`);
        visit(action);
      }
    }
  };

  visit(workflow);
  return pins;
}

export interface CheckOptions {
  /** Collects every remote action the workflows use, as `checkNestedActions` takes them. */
  actions?: Set<string>;
}

/**
 * Each workflow's pins missing from its list, listed pins with no
 * dependencies entry, and pins under a dependency's `uses:` with none.
 */
export function checkActionsLock(root: string, options: CheckOptions = {}): CheckResult {
  const lock = parseActionsLock(readFileSync(join(root, LOCK_PATH), 'utf8'));
  const workflows = readdirSync(join(root, WORKFLOWS))
    .filter((name) => /\.ya?ml$/.test(name))
    .sort()
    .map((name) => `${WORKFLOWS}/${name}`);
  const problems: Problem[] = [];

  for (const workflow of workflows) {
    const listed = lock.workflows.get(workflow) ?? new Set<string>();
    for (const [pin, from] of workflowPins(root, workflow, options.actions)) {
      if (listed.has(pin)) continue;
      const others = [...from].filter((file) => file !== workflow);
      const through = others.length > 0 ? ` (through ${others.join(', ')})` : '';
      problems.push({
        file: workflow,
        message: `${workflow} uses ${pin}${through}, which its list in ${LOCK_PATH} does not pin`,
      });
    }
  }
  for (const [workflow, listed] of lock.workflows) {
    for (const pin of listed) {
      if (!lock.dependencies.has(pin)) {
        problems.push({
          file: LOCK_PATH,
          message: `${LOCK_PATH} lists ${pin} for ${workflow} but has no dependencies entry for it`,
        });
      }
    }
  }
  for (const [dependency, nested] of lock.uses) {
    for (const pin of nested) {
      if (!lock.dependencies.has(pin)) {
        problems.push({
          file: LOCK_PATH,
          message: `${LOCK_PATH} lists ${pin} under ${dependency}'s uses but has no dependencies entry for it`,
        });
      }
    }
  }
  return { workflows, problems };
}

/**
 * An action's action.yml (or action.yaml) at a commit: `repo` is
 * `owner/repo`, `path` its subfolder or ''. Undefined when there is none.
 */
export type FetchAction = (repo: string, commit: string, path: string) => Promise<string | undefined>;

/** Reads action.yml from raw.githubusercontent.com, which public repositories need no token for. */
export const fetchActionFromGitHub: FetchAction = async (repo, commit, path) => {
  for (const name of ['action.yml', 'action.yaml']) {
    const url = `https://raw.githubusercontent.com/${repo}/${commit}/${path ? `${path}/` : ''}${name}`;
    const response = await fetch(url);
    if (response.ok) return response.text();
    if (response.status !== 404) throw new Error(`${url} answered ${response.status}`);
  }
  return undefined;
};

/**
 * The actions the given remote actions use (`uses:` values, as workflows
 * write them), read from each one's action.yml at its locked commit and
 * followed through the actions they use in turn. Each one must be listed
 * under the using action's `uses:` in the lock and have a dependencies entry.
 * A remote reusable workflow (a `.yml` path) is not followed, as
 * `gh actions-lock` does not lock its actions; an action whose pin has no
 * commit in the lock is left to `checkActionsLock`.
 */
export async function checkNestedActions(
  root: string,
  actions: Iterable<string>,
  fetchAction: FetchAction = fetchActionFromGitHub,
): Promise<Problem[]> {
  const lock = parseActionsLock(readFileSync(join(root, LOCK_PATH), 'utf8'));
  const problems: Problem[] = [];
  const queue = [...new Set(actions)];
  const visited = new Set<string>();

  while (queue.length > 0) {
    const value = queue.shift()!;
    const remote = /^([^/@\s]+\/[^/@\s]+)(?:\/([^@\s]*?))?\/?@(\S+)$/.exec(value);
    if (!remote) continue;
    const [, repoName, path = '', ref] = remote;
    const repo = repoName.toLowerCase();
    if (/\.ya?ml$/.test(path)) continue;
    const pin = `${repo}@${ref}`;
    const commit = lock.commits.get(pin);
    const key = `${repo}/${path}@${ref}`;
    if (commit === undefined || visited.has(key)) continue;
    visited.add(key);

    const text = await fetchAction(repo, commit, path);
    const where = `${repo}${path ? `/${path}` : ''}@${commit.slice(0, 7)}`;
    if (text === undefined) {
      problems.push({ file: LOCK_PATH, message: `${where} has no action.yml` });
      continue;
    }
    for (const nested of usesValues(text)) {
      let uses: Uses;
      try {
        uses = classifyUses(nested);
      } catch (error) {
        throw new Error(`${where}: ${(error as Error).message}`);
      }
      if (uses.kind !== 'action') continue;
      if (!lock.uses.get(pin)?.has(uses.pin)) {
        problems.push({
          file: LOCK_PATH,
          message: `${pin} uses ${uses.pin} (in ${where}), which ${LOCK_PATH} does not list under its uses`,
        });
      }
      // A listed pin without a dependencies entry is checkActionsLock's to report.
      queue.push(nested);
    }
  }
  return problems;
}
