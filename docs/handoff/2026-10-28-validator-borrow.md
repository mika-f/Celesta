# Validator borrows the project (2026-10-28)

`Project::validate` no longer deep-copies `characters` and `tracks` before
walking them (issue #577). The validator holds `&'project Project`, so
`Validator::validate` copies that shared reference into a local
(`let project = self.project;`) and iterates `project.characters` and
`project.tracks` while calling `&mut self` helpers. The reference does not
borrow `self`, so the loops compile without cloning.

## What changed

- `crates/project/src/validation.rs`: the two `clone()` calls are gone; the
  loops iterate the borrowed `BTreeMap` and `Vec`. Iteration order is the
  same as before (`BTreeMap` key order for characters, index order for
  tracks and items), so errors keep their content and order.
- New test `reports_character_and_track_errors_in_document_order` pins the
  full error list for a project with a settings error, two character errors
  (declared out of key order) and two track errors. It passes on both the old
  clone-based code and the new code.

## Checks

```bash
cargo test -p celesta-project
cargo clippy -p celesta-project --all-targets -- -D warnings
cargo fmt --all -- --check
```

No user-facing behavior or API changed, so the user guide and the Agent Skill
are untouched. Performance was not measured; validation runs once per
`Evaluator::new`, not per frame.
