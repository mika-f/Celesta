use std::path::Path;

use celesta_source_edit::{
    CompositionLength, EditTarget, EditValue, LiteralForm, LockReason, SceneList, Span,
    composition_length, parses_file, scene_lists,
};

fn span_text(text: &str, span: Span) -> &str {
    &text[span.start as usize..span.end as usize]
}

fn number_at(text: &str, target: &EditTarget) -> (String, f64) {
    match target {
        EditTarget::Literal {
            span,
            value: EditValue::Number(value),
            form: LiteralForm::Number,
        } => (span_text(text, *span).to_string(), *value),
        other => panic!("not a number literal: {other:?}"),
    }
}

fn ids(list: &SceneList) -> Vec<&str> {
    list.entries.iter().map(|entry| entry.id.as_str()).collect()
}

fn lists(text: &str) -> Vec<SceneList> {
    scene_lists(text, Path::new("timeline.ts"))
}

/// `body` after an import of `defineScenes` from `@celesta.studio/react`.
fn imported(body: &str) -> String {
    format!("import {{ defineScenes }} from '@celesta.studio/react';\n{body}")
}

#[test]
fn only_the_celesta_define_scenes_makes_a_list() {
    let list = "([{ id: 'a', durationInFrames: 1 }]);\n";
    for text in [
        format!(
            "import {{ defineScenes as scenes }} from '@celesta.studio/react';\nexport const S = scenes{list}"
        ),
        format!(
            "import * as C from '@celesta.studio/react';\nexport const S = C.defineScenes{list}"
        ),
        // The scope before `@celesta.studio`.
        format!(
            "import {{ defineScenes }} from '@celesta/react';\nexport const S = defineScenes{list}"
        ),
    ] {
        assert_eq!(lists(&text).len(), 1, "{text}");
    }
    for text in [
        // Not imported, or from elsewhere.
        format!("export const S = defineScenes{list}"),
        format!("import {{ defineScenes }} from './scenes';\nexport const S = defineScenes{list}"),
        // A function of the file's own.
        format!("const defineScenes = (s) => s;\nexport const S = defineScenes{list}"),
        // Shadowed where it is called.
        format!(
            "import {{ defineScenes }} from '@celesta.studio/react';\nfunction f(defineScenes) {{\n  return defineScenes{list}}}\n"
        ),
        format!("import * as C from './celesta';\nexport const S = C.defineScenes{list}"),
    ] {
        assert!(lists(&text).is_empty(), "{text}");
    }
}

#[test]
fn finds_scene_lists_with_ids_in_order() {
    // `.ts`, not TSX: `<T>(x: T) => x` and `<number>v` parse only as TS.
    let text = "import { defineScenes } from '@celesta.studio/react';\n\nconst id = <T>(x: T) => x;\nconst v: unknown = 1;\nconst n = <number>v;\n\nexport const SCENES = defineScenes([\n  { id: 'intro', durationInFrames: 90 },\n  { id: \"main\", name: 'MAIN', durationInFrames: 120 },\n  { id: 'outro', durationInFrames: 60 },\n]);\n";
    let found = lists(text);
    assert_eq!(found.len(), 1);
    let list = &found[0];
    assert_eq!(list.binding.as_deref(), Some("SCENES"));
    assert_eq!(ids(list), ["intro", "main", "outro"]);
    assert_eq!(
        number_at(text, &list.entries[0].duration),
        ("90".into(), 90.0)
    );
    assert_eq!(
        number_at(text, &list.entries[1].duration),
        ("120".into(), 120.0)
    );
    assert!(list.entries[0].inputs.is_empty());

    // Type arguments on the call, and a list no binding names.
    let text = "import { defineScenes, type SceneEntry } from '@celesta.studio/react';\nexport default defineScenes<SceneEntry & { chapter?: number }>([\n  { id: 'open', durationInFrames: 30 },\n]);\n";
    let found = scene_lists(text, Path::new("timeline.tsx"));
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].binding, None);
    assert_eq!(ids(&found[0]), ["open"]);

    // Several lists, in source order.
    let text = &imported(
        "const A = defineScenes([{ id: 'a', durationInFrames: 1 }]);\nlet B = defineScenes([{ id: 'b', durationInFrames: 2 }]);\n",
    );
    let found = lists(text);
    assert_eq!(found.len(), 2);
    assert_eq!(found[0].binding.as_deref(), Some("A"));
    assert_eq!(ids(&found[0]), ["a"]);
    // Only a `const` names a list.
    assert_eq!(found[1].binding, None);
    assert_eq!(ids(&found[1]), ["b"]);
}

#[test]
fn a_list_with_a_spread_keeps_the_entries_it_can_read() {
    // As `examples/delta/v0.8.0/timeline.ts` writes it.
    let text = &imported(
        "export const SCENES = defineScenes([\n  { id: 'open', durationInFrames: 30 },\n  ...CHAPTERS.map((c) => ({ id: c.key, durationInFrames: c.len })),\n  { id: 'outro', durationInFrames: 60 },\n]);\n",
    );
    let found = lists(text);
    assert_eq!(found.len(), 1);
    assert_eq!(ids(&found[0]), ["open", "outro"]);
    assert_eq!(found[0].unread, [1]);
    assert_eq!(
        number_at(text, &found[0].entries[1].duration),
        ("60".into(), 60.0)
    );

    // Anything but object literals with string-literal ids, by the number
    // of entries read before it.
    for (list, read, unread) in [
        ("CHAPTERS", vec![], vec![0]),
        (
            "CHAPTERS.map((c) => ({ id: c.key, durationInFrames: c.len }))",
            vec![],
            vec![0],
        ),
        (
            "[{ id: 'a', durationInFrames: 1 }, ENTRY]",
            vec!["a"],
            vec![1],
        ),
        ("[{ id: `a`, durationInFrames: 1 }]", vec![], vec![0]),
        ("[{ id: KEY, durationInFrames: 1 }]", vec![], vec![0]),
        ("[{ durationInFrames: 1 }]", vec![], vec![0]),
        (
            "[{ ...BASE, id: 'a', durationInFrames: 1 }]",
            vec![],
            vec![0],
        ),
        (
            "[{ id: 'a', durationInFrames: 1 }, , { id: 'b', durationInFrames: 1 }]",
            vec!["a", "b"],
            vec![1],
        ),
        (
            "[...A, { id: 'a', durationInFrames: 1 }, ...B]",
            vec!["a"],
            vec![0, 1],
        ),
    ] {
        let text = imported(&format!("export const SCENES = defineScenes({list});\n"));
        let found = lists(&text);
        assert_eq!(found.len(), 1, "{list}");
        assert_eq!(ids(&found[0]), read, "{list}");
        assert_eq!(found[0].unread, unread, "{list}");
    }

    // Every entry read.
    assert!(
        lists(&imported(
            "export const S = defineScenes([{ id: 'a', durationInFrames: 1 }]);\n"
        ))[0]
            .unread
            .is_empty()
    );

    // A call of something else.
    let text = &imported("export const SCENES = makeScenes([{ id: 'a', durationInFrames: 1 }]);\n");
    assert!(lists(text).is_empty());
}

#[test]
fn a_list_knows_the_names_it_is_exported_under() {
    let exports = |body: &str| -> Vec<Vec<String>> {
        lists(&imported(body))
            .into_iter()
            .map(|list| list.exports)
            .collect()
    };
    let list = "defineScenes([{ id: 'a', durationInFrames: 1 }])";
    assert_eq!(exports(&format!("export const S = {list};\n")), [["S"]]);
    assert_eq!(
        exports(&format!(
            "const S = {list};\nexport {{ S, S as SCENES }};\n"
        )),
        [["S", "SCENES"]]
    );
    assert_eq!(
        exports(&format!("const S = {list};\nexport default S;\n")),
        [["default"]]
    );
    assert_eq!(
        exports(&format!("const S = {list};\nexport {{ S as default }};\n")),
        [["default"]]
    );
    assert_eq!(exports(&format!("export default {list};\n")), [["default"]]);
    assert_eq!(
        exports(&format!("export default ({list}) satisfies object;\n")),
        [["default"]]
    );
    // A nested `const` of the exported name is another binding.
    let found = lists(&imported(&format!(
        "export const SCENES = {list};\nfunction f() {{\n  const SCENES = {list};\n  return SCENES;\n}}\n"
    )));
    assert_eq!(found[0].exports, ["SCENES"]);
    assert!(found[1].exports.is_empty());
    // Not exported; another module's `S`; a type-only export.
    let none: [Vec<String>; 1] = [vec![]];
    assert_eq!(exports(&format!("const S = {list};\n")), none);
    assert_eq!(
        exports(&format!(
            "const S = {list};\nexport {{ S }} from './other';\n"
        )),
        none
    );
    assert_eq!(
        exports(&format!("const S = {list};\nexport type {{ S }};\n")),
        none
    );
}

#[test]
fn a_const_duration_resolves_to_the_const() {
    let text = &imported(
        "const INTRO = 90;\nexport const SCENES = defineScenes([\n  { id: 'intro', durationInFrames: INTRO },\n]);\n",
    );
    let list = &lists(text)[0];
    assert_eq!(
        number_at(text, &list.entries[0].duration),
        ("90".into(), 90.0)
    );

    // A const read twice is shared.
    let text = &imported(
        "const LEN = 90;\nexport const SCENES = defineScenes([\n  { id: 'a', durationInFrames: LEN },\n  { id: 'b', durationInFrames: LEN },\n]);\n",
    );
    assert_eq!(
        lists(text)[0].entries[0].duration,
        EditTarget::Locked(LockReason::SharedBinding {
            name: "LEN".into(),
            uses: 2,
        })
    );

    // An imported name does not resolve.
    let text = &imported(
        "import { LEN } from './constants';\nexport const SCENES = defineScenes([{ id: 'a', durationInFrames: LEN }]);\n",
    );
    assert_eq!(
        lists(text)[0].entries[0].duration,
        EditTarget::Locked(LockReason::UnresolvedIdentifier { name: "LEN".into() })
    );
}

#[test]
fn an_exported_const_duration_is_locked() {
    // Other files may read it: writing it would move them too.
    let text = &imported(
        "export const OUTRO = 60;\nexport const SCENES = defineScenes([\n  { id: 'outro', durationInFrames: OUTRO },\n]);\n",
    );
    assert_eq!(
        lists(text)[0].entries[0].duration,
        EditTarget::Locked(LockReason::Exported {
            name: "OUTRO".into(),
        })
    );

    // Exported by a list of names, under its own name or another.
    for export in ["export { OUTRO };", "export { OUTRO as END };"] {
        let text = imported(&format!(
            "const OUTRO = 60;\n{export}\nexport const SCENES = defineScenes([\n  {{ id: 'outro', durationInFrames: OUTRO }},\n]);\n"
        ));
        assert_eq!(
            lists(&text)[0].entries[0].duration,
            EditTarget::Locked(LockReason::Exported {
                name: "OUTRO".into(),
            }),
            "{export}"
        );
    }

    // Read twice too, it is exported first.
    let text = &imported(
        "export const LEN = 60;\nexport const SCENES = defineScenes([\n  { id: 'a', durationInFrames: LEN },\n  { id: 'b', durationInFrames: LEN },\n]);\n",
    );
    assert_eq!(
        lists(text)[0].entries[0].duration,
        EditTarget::Locked(LockReason::Exported { name: "LEN".into() })
    );
    // Other locks stay.
    let text = &imported(
        "export const LEN = spring(1);\nexport const SCENES = defineScenes([\n  { id: 'a', durationInFrames: LEN },\n]);\n",
    );
    assert_eq!(
        lists(text)[0].entries[0].duration,
        EditTarget::Locked(LockReason::UnsupportedCall {
            callee: "spring".into(),
        })
    );

    // `export { OUTRO } from` exports another module's `OUTRO`: the file's
    // own stays editable.
    let text = &imported(
        "const OUTRO = 60;\nexport { OUTRO } from './other';\nexport const SCENES = defineScenes([\n  { id: 'outro', durationInFrames: OUTRO },\n]);\n",
    );
    assert_eq!(
        number_at(text, &lists(text)[0].entries[0].duration),
        ("60".into(), 60.0)
    );

    // Another export of the file leaves the const editable.
    let text = &imported(
        "const OUTRO = 60;\nexport const END = 1;\nexport const SCENES = defineScenes([\n  { id: 'outro', durationInFrames: OUTRO },\n]);\n",
    );
    assert_eq!(
        number_at(text, &lists(text)[0].entries[0].duration),
        ("60".into(), 60.0)
    );
}

#[test]
fn a_getter_a_method_or_a_later_key_is_read_as_the_runtime_reads_it() {
    // A getter or a method is not a literal.
    for (entry, source) in [
        (
            "{ id: 'a', get durationInFrames() { return 30; } }",
            "get durationInFrames() { return 30; }",
        ),
        (
            "{ id: 'a', durationInFrames() { return 30; } }",
            "durationInFrames() { return 30; }",
        ),
    ] {
        let text = imported(&format!("export const S = defineScenes([{entry}]);\n"));
        assert_eq!(
            lists(&text)[0].entries[0].duration,
            EditTarget::Locked(LockReason::Expression {
                source: source.into(),
            }),
            "{entry}"
        );
    }
    // A duplicate key: the later one wins, as at runtime.
    let text = &imported(
        "export const S = defineScenes([{ id: 'a', durationInFrames: 30, durationInFrames: 45 }]);\n",
    );
    assert_eq!(
        number_at(text, &lists(text)[0].entries[0].duration),
        ("45".into(), 45.0)
    );
    let text =
        &imported("export const S = defineScenes([{ id: 'a', id: 'b', durationInFrames: 30 }]);\n");
    assert_eq!(ids(&lists(text)[0]), ["b"]);
}

#[test]
fn an_entry_without_a_duration_is_locked() {
    let text = &imported(
        "export const SCENES = defineScenes([\n  { id: 'intro' },\n  { id: 'outro', durationInFrames: 60 },\n]);\n",
    );
    let list = &lists(text)[0];
    assert_eq!(ids(list), ["intro", "outro"]);
    assert_eq!(
        list.entries[0].duration,
        EditTarget::Locked(LockReason::NoSceneDuration { id: "intro".into() })
    );
    assert!(list.entries[0].inputs.is_empty());
}

#[test]
fn an_expression_duration_is_locked_with_its_inputs() {
    let text = &imported(
        "const BAR = 30;\nexport const SCENES = defineScenes([\n  { id: 'intro', durationInFrames: BAR * 3 },\n  { id: 'outro', durationInFrames: 60 },\n]);\n",
    );
    let entry = &lists(text)[0].entries[0];
    assert_eq!(
        entry.duration,
        EditTarget::Locked(LockReason::Expression {
            source: "BAR * 3".into(),
        })
    );
    assert_eq!(entry.inputs.len(), 1);
    assert_eq!(entry.inputs[0].0, "BAR");
    assert_eq!(number_at(text, &entry.inputs[0].1), ("30".into(), 30.0));
    assert!(lists(text)[0].entries[1].inputs.is_empty());

    // `S.words - S.intro`: the object is the input, and is not a number.
    let text = &imported(
        "export const S = { intro: 0, words: 40 } as const;\nexport const SCENES = defineScenes([\n  { id: 'intro', durationInFrames: S.words - S.intro },\n]);\n",
    );
    let entry = &lists(text)[0].entries[0];
    assert_eq!(
        entry.duration,
        EditTarget::Locked(LockReason::Expression {
            source: "S.words - S.intro".into(),
        })
    );
    assert_eq!(
        entry.inputs,
        vec![(
            "S".to_string(),
            EditTarget::Locked(LockReason::UnresolvedIdentifier { name: "S".into() })
        )]
    );
}

fn length(text: &str) -> Option<CompositionLength> {
    composition_length(text, Path::new("film.tsx"))
}

fn film(import: &str, before: &str, duration: &str) -> String {
    format!(
        "import {{ Composition }} from '@celesta.studio/react';\n{import}\n{before}\nexport const Film = () => (\n  <Composition width={{1920}} height={{1080}} fps={{30}} durationInFrames={duration}>\n  </Composition>\n);\n"
    )
}

#[test]
fn composition_length_reads_the_list_reference() {
    let text = film(
        "",
        "const SCENES = defineScenes([{ id: 'a', durationInFrames: 30 }]);",
        "{SCENES.durationInFrames}",
    );
    assert_eq!(
        length(&text),
        Some(CompositionLength::ListDuration {
            module: None,
            name: "SCENES".into(),
        })
    );

    let text = film(
        "import { SCENES } from './timeline';",
        "",
        "{SCENES.durationInFrames}",
    );
    assert_eq!(
        length(&text),
        Some(CompositionLength::ListDuration {
            module: Some("./timeline".into()),
            name: "SCENES".into(),
        })
    );

    let text = film(
        "import { SCENES as S } from './timeline';",
        "",
        "{S.durationInFrames}",
    );
    assert_eq!(
        length(&text),
        Some(CompositionLength::ListDuration {
            module: Some("./timeline".into()),
            name: "SCENES".into(),
        })
    );

    // A default import names the module's `default` export, which
    // `SceneList::exports` lists as `default`.
    let text = film("import S from './timeline';", "", "{S.durationInFrames}");
    assert_eq!(
        length(&text),
        Some(CompositionLength::ListDuration {
            module: Some("./timeline".into()),
            name: "default".into(),
        })
    );
    // A namespace import doesn't name the binding.
    let text = film(
        "import * as T from './timeline';",
        "",
        "{T.durationInFrames}",
    );
    assert_eq!(length(&text), Some(CompositionLength::Other));
    // The import shadowed in the file.
    let text = film(
        "import { SCENES } from './timeline';",
        "function f(SCENES) { return SCENES; }",
        "{SCENES.durationInFrames}",
    );
    assert_eq!(length(&text), Some(CompositionLength::Other));
    // A spread after the attribute may set it.
    let text = "import { Composition } from '@celesta.studio/react';\nimport { SCENES } from './timeline';\nexport const Film = () => <Composition durationInFrames={SCENES.durationInFrames} {...rest} />;\n";
    assert_eq!(length(text), Some(CompositionLength::Other));

    assert_eq!(
        length(&film("", "", "{300}")),
        Some(CompositionLength::Other)
    );
    // Neither a binding of this file nor an import.
    assert_eq!(
        length(&film("", "", "{SCENES.durationInFrames}")),
        Some(CompositionLength::Other)
    );
    // Another property of the list.
    assert_eq!(
        length(&film(
            "import { SCENES } from './timeline';",
            "",
            "{SCENES.length}"
        )),
        Some(CompositionLength::Other)
    );

    let text = "export const Film = () => <Group />;\n";
    assert_eq!(length(text), None);
}

#[test]
fn parses_file_by_extension() {
    let text = "const v: unknown = 1;\nconst n = <number>v;\n";
    assert!(parses_file(text, Path::new("timeline.ts")));
    assert!(!parses_file(text, Path::new("timeline.tsx")));
    // TSX when the extension says nothing.
    assert!(!parses_file(text, Path::new("timeline")));
    assert!(parses_file(
        "export const A = () => <Rect />;\n",
        Path::new("timeline")
    ));
}
