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

#[test]
fn finds_scene_lists_with_ids_in_order() {
    // `.ts`, not TSX: `<T>(x: T) => x` and `<number>v` parse only as TS.
    let text = "import { defineScenes } from '@celesta/react';\n\nconst id = <T>(x: T) => x;\nconst v: unknown = 1;\nconst n = <number>v;\n\nexport const SCENES = defineScenes([\n  { id: 'intro', durationInFrames: 90 },\n  { id: \"main\", name: 'MAIN', durationInFrames: 120 },\n  { id: 'outro', durationInFrames: 60 },\n]);\n";
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
    let text = "import { defineScenes, type SceneEntry } from '@celesta/react';\nexport default defineScenes<SceneEntry & { chapter?: number }>([\n  { id: 'open', durationInFrames: 30 },\n]);\n";
    let found = scene_lists(text, Path::new("timeline.tsx"));
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].binding, None);
    assert_eq!(ids(&found[0]), ["open"]);

    // Several lists, in source order.
    let text = "const A = defineScenes([{ id: 'a', durationInFrames: 1 }]);\nlet B = defineScenes([{ id: 'b', durationInFrames: 2 }]);\n";
    let found = lists(text);
    assert_eq!(found.len(), 2);
    assert_eq!(found[0].binding.as_deref(), Some("A"));
    assert_eq!(ids(&found[0]), ["a"]);
    // Only a `const` names a list.
    assert_eq!(found[1].binding, None);
    assert_eq!(ids(&found[1]), ["b"]);
}

#[test]
fn a_list_with_a_spread_is_skipped() {
    let text = "export const SCENES = defineScenes([\n  { id: 'open', durationInFrames: 30 },\n  ...CHAPTERS.map((c) => ({ id: c.key, durationInFrames: c.len })),\n]);\n";
    assert!(lists(text).is_empty());

    // Anything but object literals with string-literal ids.
    for list in [
        "CHAPTERS",
        "CHAPTERS.map((c) => ({ id: c.key, durationInFrames: c.len }))",
        "[{ id: 'a', durationInFrames: 1 }, ENTRY]",
        "[{ id: `a`, durationInFrames: 1 }]",
        "[{ id: KEY, durationInFrames: 1 }]",
        "[{ durationInFrames: 1 }]",
        "[{ ...BASE, id: 'a', durationInFrames: 1 }]",
        "[{ id: 'a', durationInFrames: 1 }, , { id: 'b', durationInFrames: 1 }]",
    ] {
        let text = format!("export const SCENES = defineScenes({list});\n");
        assert!(lists(&text).is_empty(), "{list}");
    }

    // A call of something else.
    let text = "export const SCENES = makeScenes([{ id: 'a', durationInFrames: 1 }]);\n";
    assert!(lists(text).is_empty());
}

#[test]
fn a_const_duration_resolves_to_the_const() {
    let text = "const INTRO = 90;\nexport const OUTRO = 60;\nexport const SCENES = defineScenes([\n  { id: 'intro', durationInFrames: INTRO },\n  { id: 'outro', durationInFrames: OUTRO },\n]);\n";
    let list = &lists(text)[0];
    assert_eq!(
        number_at(text, &list.entries[0].duration),
        ("90".into(), 90.0)
    );
    // An exported const too.
    assert_eq!(
        number_at(text, &list.entries[1].duration),
        ("60".into(), 60.0)
    );

    // A const read twice is shared.
    let text = "const LEN = 90;\nexport const SCENES = defineScenes([\n  { id: 'a', durationInFrames: LEN },\n  { id: 'b', durationInFrames: LEN },\n]);\n";
    assert_eq!(
        lists(text)[0].entries[0].duration,
        EditTarget::Locked(LockReason::SharedBinding {
            name: "LEN".into(),
            uses: 2,
        })
    );

    // An imported name does not resolve.
    let text = "import { LEN } from './constants';\nexport const SCENES = defineScenes([{ id: 'a', durationInFrames: LEN }]);\n";
    assert_eq!(
        lists(text)[0].entries[0].duration,
        EditTarget::Locked(LockReason::UnresolvedIdentifier { name: "LEN".into() })
    );
}

#[test]
fn an_expression_duration_is_locked_with_its_inputs() {
    let text = "const BAR = 30;\nexport const SCENES = defineScenes([\n  { id: 'intro', durationInFrames: BAR * 3 },\n  { id: 'outro', durationInFrames: 60 },\n]);\n";
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
    let text = "export const S = { intro: 0, words: 40 } as const;\nexport const SCENES = defineScenes([\n  { id: 'intro', durationInFrames: S.words - S.intro },\n]);\n";
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
        "import {{ Composition }} from '@celesta/react';\n{import}\n{before}\nexport const Film = () => (\n  <Composition width={{1920}} height={{1080}} fps={{30}} durationInFrames={duration}>\n  </Composition>\n);\n"
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
