# Package MCP tool assertion

The main Windows package smoke job failed at `scripts/test-package.mjs`'s
`tools/list` assertion: the expected list omitted `check_quality`,
`list_comments`, `reply_to_comment`, and `resolve_comment` from #880/#881.
Add those four names, preserving the sorted, exact assertion of all 13 tools.
No MCP registration or runtime behavior changes.

Checked every `#[tool(...)]` registration in `crates/mcp/src/server.rs`
against the updated array with Node assertions: exactly 13 names, equal and
sorted, with no extras or missing tools. `node --check scripts/test-package.mjs`
and `git diff --check` pass. The packaged Windows smoke test was not rerun on
this Linux machine; the supplied Windows log shows precisely those four
additional actual tools and no other difference.
