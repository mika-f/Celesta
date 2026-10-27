# Weekly Renovate lockfile maintenance (2026-10-28)

## Specification and implementation

Enable weekly lockfile refreshes so transitive dependencies can receive updates
within the constraints in the package manifests. Add the official
`:maintainLockFilesWeekly` preset alongside `config:recommended` in
`renovate.json`. The preset schedules maintenance for early Monday mornings.

This refresh is separate from alert-specific security remediation. Renovate's
`minimumReleaseAge` does not filter transitive updates made by the package
manager during lockfile maintenance; the package manager's own release-age
policy applies. See the [preset documentation](https://docs.renovatebot.com/presets-default/#maintainlockfilesweekly)
and [transitive dependency behavior](https://docs.renovatebot.com/key-concepts/minimum-release-age/#what-happens-to-transitive-dependencies).

## Checks

- Parsed `renovate.json` with Node.js and asserted that its `extends` contains
  exactly `config:recommended` and `:maintainLockFilesWeekly`.
- Compared the parsed configuration against the original: the preset addition
  is the only configuration change.
- Confirmed the preset and its weekly schedule against Renovate's official docs.
- `git diff --check` passed.

No application code or lockfiles change in this task.
