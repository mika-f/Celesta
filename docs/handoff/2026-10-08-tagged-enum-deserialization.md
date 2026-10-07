# Tagged enums without buffering (2026-10-08)

`LayerContent`, `PathCommand`, `AssetLocation` and `Paint` are tagged by a
`"type"` field. serde's derived `#[serde(tag = "type")]` deserializer reads
each such object into an intermediate tree before it picks the variant, and
a group's children go through that tree again at every level. Profiling the
bridge's JSON decoding of a NEBULA frame put about a quarter of it there,
more than number parsing.

`crates/composition/src/tagged.rs` adds `TypeTagged`, a deserializer adapter. Each
enum now derives `Deserialize` on an externally tagged mirror
(`#[serde(remote = "...")]`, `LayerContentDef` and so on) and deserializes
it through `TypeTagged`, which reads the variant from the `type` field and
hands the rest of the object straight to the variant's fields. `Serialize`,
the ts-rs bindings and the JSON format are unchanged.

- `@celesta/react` and `Serialize` write `type` first (all 416,640 tagged
  objects in 120 NEBULA frames). When `type` comes later, as in a
  hand-written project file, the fields before it are buffered as
  `serde_json::Value`s; errors in those values lose their line and column.
- A repeated `type` key is still rejected as a duplicate field, as the
  derived deserializer did.
- The mirrors repeat each variant's fields. The compiler checks names and
  types against the real enum, but serde attributes (`default`,
  `default = "default_miter_limit"`) must be kept in step by hand.
  `every_tagged_variant_round_trips` round trips every variant with its
  optional fields left out and set, through a string (`type` first) and a
  `serde_json::Value` (`type` among sorted keys), so a mirror missing a
  `default` fails it.
- Tests: `tagged_content_parses_with_its_type_first_or_later`,
  `tagged_content_fills_omitted_fields_with_their_defaults`,
  `tagged_content_reports_a_missing_or_unknown_type`,
  `tagged_content_rejects_a_repeated_type`, `nested_groups_round_trip` and
  `every_tagged_variant_round_trips`.

Measured on an Apple M4, Node 26, release build, with
`crates/react-bridge/examples/protocol-bench.rs <entry> 300 60` (30 warmup
frames from frame 60, then frames 90 to 389 measured; medians of three
alternating runs):

| Entry | Before | After |
| --- | --- | --- |
| `examples/versus/bench/celesta/nebula.tsx` | 3.74 ms | 3.47 ms |
| `examples/spectra/film.tsx` | 0.79 ms | 0.72 ms |
| `examples/reel/film.tsx` | 1.61 ms | 1.38 ms |

Decoding a NEBULA frame alone (about 469 KB of JSON) went from 1.25 ms to
0.86 ms.

Considered and dropped: sending frames as CBOR or MessagePack. The Rust CBOR
crates that are maintained (ciborium, minicbor, cbor4ii) reject the integers
`cbor-x` writes for integral numbers where an `f64` is expected. MessagePack
(msgpackr, rmp-serde) cut NEBULA's round trip by 12% but made `reel` 25%
slower, since `JSON.stringify` is faster than msgpackr for small frames, and
it needs `undefined` skipped explicitly to match JSON.
