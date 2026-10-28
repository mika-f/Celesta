When updating a pull request branch with changes from its base branch, rebase the pull request branch onto the base branch.

* Read [`HANDOFF.md`](HANDOFF.md) for the architecture, runtime constraints, and validation commands. Record a task's specification, implementation details, and checks in a new `docs/handoff/<date>-<task>.md` note rather than in `HANDOFF.md`.
* For rendering-related changes, attach screenshots of the same scene taken before and after the rendering change.
* When a new feature supersedes the content of an existing example, update the corresponding example scene in the same pull request.
* When user-facing behavior or a composition API changes, update the user guide in both `packages/website/content/en/` and `packages/website/content/ja/`, and the Celesta Agent Skill in `skills/celesta/` where it covers that API.
* Use Vitest for JavaScript and TypeScript tests and write test code in TypeScript. Rust code keeps its tests in Rust (`cargo test`).

Never amend commits. Record further changes as a new commit.

If an amend is the only way to finish the task, ask the user before running `git commit --amend`. Do not amend on your own judgment. Ask only in these situations:

* The latest commit is unpushed and contains a secret. Ask before amending it to remove the secret. If that commit is already pushed, do not amend it.
* An interactive rebase has stopped to edit a commit (`git status` reports that you are currently editing a commit during a rebase). Ask before amending that commit, then run `git rebase --continue`. A rebase, cherry-pick, revert, or merge stopped for conflicts is not this case; continue that operation instead of amending.

<!-- BEGIN:turborepo-agent-rules -->

# This is NOT the Turborepo you know

Turborepo configuration, task behavior, and CLI commands can vary between installed versions and may differ from your training data. Resolve the `turbo` package from this file's directory or relevant workspace; in monorepos, it may not be visible from the repository root. For example, run `node -p "require.resolve('turbo/package.json')"` from a workspace that depends on `turbo`.

Read `docs/README.md` inside that installed package first, then read the relevant pages from its `docs/` directory before changing Turborepo configuration or commands. Heed deprecation notices. These bundled docs match the installed package version and are available without network access.

This block is written and re-added by `turbo` before repository-scoped commands when an AI agent is detected. In the Turborepo source repository, its template is defined in `crates/turborepo-cli/src/cli/agent_guidance.rs`. Removing the managed block while updates are enabled means a later qualifying invocation will add it again. Set `"agentGuidance": false` in the root `turbo.json` or `turbo.jsonc` to opt out; this does not remove an existing block. Keep the block committed with your work to avoid an uncommitted change on the next agent invocation.
<!-- END:turborepo-agent-rules -->
