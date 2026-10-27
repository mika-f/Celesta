use celesta_source_edit::{
    EditTarget, EditValue, ElementRef, LiteralForm, LockReason, Span, TimingKind, element_at,
    later_series_items, resolve_unchecked, timing_kind,
};

/// The element whose `<` is the first `needle` in `text` (ASCII only).
fn element(text: &str, needle: &str) -> ElementRef {
    let offset = text.find(needle).unwrap();
    let before = &text[..offset];
    let line = before.matches('\n').count() as u32 + 1;
    let column = (offset - before.rfind('\n').map_or(0, |i| i + 1)) as u32 + 1;
    element_at(text, line, column).unwrap()
}

fn span_text(text: &str, span: Span) -> &str {
    &text[span.start as usize..span.end as usize]
}

/// The kind of the first `<{tag}` in a scene importing `import`.
fn kind(import: &str, body: &str, tag: &str) -> Option<TimingKind> {
    let text = format!("{import}\nexport const Scene = () => (\n  {body}\n);\n");
    timing_kind(&text, &element(&text, &format!("<{tag}")))
}

#[test]
fn timing_kind_reads_the_import() {
    let named = "import { Rect, Sequence, Series } from '@celesta/react';";
    assert_eq!(
        kind(named, "<Sequence from={30}><Rect /></Sequence>", "Sequence"),
        Some(TimingKind::Sequence)
    );
    assert_eq!(
        kind(
            "import { Sequence as Seq } from '@celesta/react';",
            "<Seq from={30} />",
            "Seq"
        ),
        Some(TimingKind::Sequence)
    );
    assert_eq!(
        kind(
            "import * as C from '@celesta/react';",
            "<C.Sequence from={30} />",
            "C.Sequence"
        ),
        Some(TimingKind::Sequence)
    );
    assert_eq!(
        kind(
            named,
            "<Series><Series.Sequence durationInFrames={30} /></Series>",
            "Series.Sequence"
        ),
        Some(TimingKind::SeriesSequence)
    );
    assert_eq!(
        kind(
            "import { Series as S } from '@celesta/react';",
            "<S><S.Sequence durationInFrames={30} /></S>",
            "S.Sequence"
        ),
        Some(TimingKind::SeriesSequence)
    );
    assert_eq!(
        kind(
            "import * as C from '@celesta/react';",
            "<C.Series><C.Series.Sequence durationInFrames={30} /></C.Series>",
            "C.Series.Sequence"
        ),
        Some(TimingKind::SeriesSequence)
    );
    // Not a timing wrapper: other tags, the `Series` itself.
    assert_eq!(kind(named, "<Rect />", "Rect"), None);
    assert_eq!(kind(named, "<Series />", "Series"), None);

    // A local component named `Sequence` is not the wrapper.
    let text = "import { Rect } from '@celesta/react';\nfunction Sequence() {\n  return <Rect />;\n}\nexport const Scene = () => <Sequence from={30} />;\n";
    assert_eq!(timing_kind(text, &element(text, "<Sequence")), None);

    // Nor one imported from elsewhere, or as a type only.
    assert_eq!(
        kind(
            "import { Sequence } from './sequence';",
            "<Sequence />",
            "Sequence"
        ),
        None
    );
    assert_eq!(
        kind(
            "import type { Sequence } from '@celesta/react';",
            "<Sequence />",
            "Sequence"
        ),
        None
    );
}

#[test]
fn later_series_items_are_the_following_siblings() {
    let text = "import { Rect, Series } from '@celesta/react';\nexport const Scene = () => (\n  <Series>\n    <Series.Sequence durationInFrames={30}><Rect /></Series.Sequence>\n    <Series.Sequence durationInFrames={40}><Rect /></Series.Sequence>\n    <Series.Sequence durationInFrames={50}><Rect /></Series.Sequence>\n  </Series>\n);\n";
    let items: Vec<ElementRef> = text
        .match_indices("<Series.Sequence")
        .map(|(offset, _)| {
            let before = &text[..offset];
            let line = before.matches('\n').count() as u32 + 1;
            let column = (offset - before.rfind('\n').map_or(0, |i| i + 1)) as u32 + 1;
            element_at(text, line, column).unwrap()
        })
        .collect();
    assert_eq!(items.len(), 3);

    let later = later_series_items(text, &items[0]);
    assert_eq!(later, vec![(5, 5), (6, 5)]);
    // Each position is what `element_at` takes.
    let found: Vec<ElementRef> = later
        .iter()
        .map(|&(line, column)| element_at(text, line, column).unwrap())
        .collect();
    assert_eq!(found, items[1..]);

    assert_eq!(later_series_items(text, &items[1]), vec![(6, 5)]);
    assert_eq!(
        later_series_items(text, &items[2]),
        Vec::<(u32, u32)>::new()
    );

    // Columns count UTF-16 code units.
    let text = "import { Series } from '@celesta/react';\nexport const Scene = () => <Series><Series.Sequence durationInFrames={1} />{/* 🎬 */}<Series.Sequence durationInFrames={2} /></Series>;\n";
    let first = element(text, "<Series.Sequence");
    let later = later_series_items(text, &first);
    let offset = text.rfind("<Series.Sequence").unwrap();
    let line_start = text[..offset].rfind('\n').unwrap() + 1;
    let column = text[line_start..offset].encode_utf16().count() as u32 + 1;
    assert_eq!(later, vec![(2, column)]);
    assert_eq!(
        element_at(text, 2, column).map(|e| e.start),
        Some(offset as u32)
    );
}

/// The first `<Sequence` of `text`, read unchecked, without a call site.
fn unchecked(text: &str, prop: &str) -> EditTarget {
    resolve_unchecked(text, &element(text, "<Sequence"), prop, None)
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

#[test]
fn resolve_unchecked_reads_written_props() {
    // A literal: the runtime reports no value for a wrapper's props, so it
    // is not compared with one (`OverriddenByParent` otherwise).
    let text = "export const Scene = () => (\n  <Sequence from={30} durationInFrames={60} />\n);\n";
    assert_eq!(
        number_at(text, &unchecked(text, "from")),
        ("30".into(), 30.0)
    );

    // A function-level const resolves to its literal.
    let text = "export const Scene = () => {\n  const START = 30;\n  return <Sequence from={START} />;\n};\n";
    assert_eq!(
        number_at(text, &unchecked(text, "from")),
        ("30".into(), 30.0)
    );

    // A parameter resolves at the call site, unchecked.
    let component = "export const Part = ({ start }) => (\n  <Sequence from={start} />\n);\n";
    let scene = "export const Scene = () => (\n  <Part start={45} />\n);\n";
    let call_site = element(scene, "<Part");
    let target = resolve_unchecked(
        component,
        &element(component, "<Sequence"),
        "from",
        Some((scene, &call_site)),
    );
    let EditTarget::PassThrough {
        call_site: found,
        param,
        target,
    } = target
    else {
        panic!("not a pass-through: {target:?}");
    };
    assert_eq!(found, call_site);
    assert_eq!(param, "start");
    assert_eq!(number_at(scene, &target), ("45".into(), 45.0));

    // An absent prop: no default is known.
    let text = "export const Scene = () => (\n  <Sequence from={30} />\n);\n";
    let sequence = element(text, "<Sequence");
    assert_eq!(
        unchecked(text, "durationInFrames"),
        EditTarget::Missing {
            insert_at: sequence.end_of_attributes,
            default: None,
        }
    );

    // An expression locks.
    let text = "export const Scene = ({ cut }) => (\n  <Sequence from={cut - 9} />\n);\n";
    assert_eq!(
        unchecked(text, "from"),
        EditTarget::Locked(LockReason::Expression {
            source: "cut - 9".into(),
        })
    );
}
