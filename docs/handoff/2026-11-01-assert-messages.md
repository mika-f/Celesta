# Assert messages that repeated the actual value (2026-11-01)

## Specification and implementation

Some tests passed the value an assertion compares as its message, as in
`assert.equal(frame.error, undefined, frame.error)`. The message argument
describes the check. It is not a place for the actual value, which the
`+ actual - expected` diff already shows. Node.js 24 and 26 both keep that
diff after a custom message, so the value still appears when a check fails.

The pattern came up in the `@types/node` 26 upgrade (#627). In 26, the
`node:assert` functions take either `message?: Error | AssertMessageFunction`
or a printf-style `message: string, ...args`, so a `string | undefined` message
fails the test type checks with TS2769. The pattern predates that upgrade, so
this change fixes it on `main` on its own. The new messages are string
literals and type check with `@types/node` 24 and 26.

The six error checks now pass a description:

- `packages/character/test/render-cli.mts`, `packages/shader/test/shader.test.mts`:
  `'a frame without an error'`.
- `packages/character/test/dialogue-series.test.mts`,
  `packages/cli/test/motion.test.mts`: `` `frame ${n} without an error` ``,
  naming the requested frame.
- `packages/character/test/voice-folder.test.mts`:
  `'a ready message without an error'`.
- `packages/code/test/code.test.mts`: `'a message without an error'`.

All test files import `node:assert/strict`, so `assert.equal` is
`assert.strictEqual` and the Legacy note on `assert.equal` does not apply.

### Audit of the other assert messages

A TypeScript-AST scan of every file that imports `node:assert` found 537
`assert.*` calls with a message. Apart from the six above, two repeated the
actual value:

- `templates/test/templates.test.ts` put `JSON.stringify(result.invalidProperties)`
  in the message of an `equal` against `undefined`. The message is now
  `` `${name}: no invalid properties` ``.
- `packages/web/test/text-runs.mjs` (plain JavaScript, so not type checked)
  passed `report ?? result.stderr`. It now checks for a report first, with
  Chrome's stderr in that message, then compares the report to `'PASS'`.

The rest pass context the check lacks: stderr next to an exit status, the
case name in a loop, or the inspected value for `assert.ok`, which has no diff.
They stay as they are.

## Checks

On Windows 11 (x64) with Node.js 26.12.1 and pnpm 12.16.1 from mise, with
`main`'s `@types/node` 24:

- `tsc -b test` in every package with a `test/tsconfig.json` passed.
- `pnpm run test` in `@celesta/shader`, `@celesta/character`, `@celesta/cli`,
  `@celesta/code` and `@celesta/templates` passed.
- `packages/web/test/text-runs.mjs` needs headless Chrome; only
  `node --check` ran on it.

On the `@types/node` 26 branch with these changes, the same type checks and
tests passed as well.
