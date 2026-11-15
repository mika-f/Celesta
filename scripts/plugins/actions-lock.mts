// The actions each workflow under .github/workflows uses, and the pins
// .github/workflows/actions.lock gives it, for scripts/check-actions-lock.mts.
//
// GitHub refuses to start a workflow whose actions, including those its local
// composite actions use, are not all in that workflow's list in the lock
// ("lockfile missing pin for ..."), and it says so only when the workflow
// runs: for plugin-release.yml and package.yml, on a release tag.
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
  /** The pins under `dependencies:`, each with its commit. */
  dependencies: Set<string>;
}

const lowerPin = (pin: string) => {
  const at = pin.indexOf('@');
  return at < 0 ? pin.toLowerCase() : pin.slice(0, at).toLowerCase() + pin.slice(at);
};

const unquote = (text: string) => text.replace(/^'(.*)'$|^"(.*)"$/, '$1$2');

/**
 * The `workflows:` and `dependencies:` maps of an actions.lock, as
 * `gh actions-lock` writes it: four spaces before a workflow or a
 * dependency, and a list item per pin below a workflow.
 */
export function parseActionsLock(text: string): ActionsLock {
  const workflows = new Map<string, Set<string>>();
  const dependencies = new Set<string>();
  let section: string | undefined;
  let workflow: Set<string> | undefined;

  for (const raw of text.split(/\r?\n/)) {
    const line = raw.replace(/\s+#.*$/, '');
    if (line.trim() === '' || line.trimStart().startsWith('#')) continue;

    const top = /^(\S[^:]*):/.exec(line);
    if (top) {
      section = top[1];
      workflow = undefined;
      continue;
    }
    const key = /^ {4}(\S.*?):\s*$/.exec(line);
    if (key) {
      const name = unquote(key[1]);
      if (section === 'workflows') {
        workflow = new Set();
        workflows.set(name, workflow);
      } else if (section === 'dependencies') {
        dependencies.add(lowerPin(name));
      }
      continue;
    }
    const item = /^\s+-\s+(\S.*?)\s*$/.exec(line);
    if (item && section === 'workflows' && workflow) workflow.add(lowerPin(unquote(item[1])));
  }
  return { workflows, dependencies };
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
 */
export function workflowPins(root: string, workflow: string): Map<string, Set<string>> {
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

/** Each workflow's pins missing from its list, and listed pins with no commit. */
export function checkActionsLock(root: string): CheckResult {
  const lock = parseActionsLock(readFileSync(join(root, LOCK_PATH), 'utf8'));
  const workflows = readdirSync(join(root, WORKFLOWS))
    .filter((name) => /\.ya?ml$/.test(name))
    .sort()
    .map((name) => `${WORKFLOWS}/${name}`);
  const problems: Problem[] = [];

  for (const workflow of workflows) {
    const listed = lock.workflows.get(workflow) ?? new Set<string>();
    for (const [pin, from] of workflowPins(root, workflow)) {
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
  return { workflows, problems };
}
