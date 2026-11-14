# celesta-registry-format

The plugin registry's client-facing file formats: `config.json`, `index/<id>.json`,
`search.json`, `advisories.json`, and the detached `<file>.sig` signatures.

This folder is the format's source of truth.

- `schema/v1/*.schema.json` are JSON Schema (draft 2020-12) definitions. Objects allow
  additional properties, because clients ignore unknown fields.
- `fixtures/valid/` holds files every schema must accept; `fixtures/invalid/<schema>-<rule>.json`
  holds files a schema must refuse. The file-name prefix picks the schema.
- `tests/schema.rs` checks both folders (`cargo test -p celesta-registry-format`).

The registry service syncs `schema/` and `fixtures/` from a pinned commit of this repository
rather than keeping its own copies. Fixture bytes are exact (`-text` in `.gitattributes`),
since later checks sign and hash them.
