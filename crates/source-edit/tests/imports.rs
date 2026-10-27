use celesta_source_edit::{
    Edit, ImportLock, ImportName, apply, ensure_import, ensure_imports, parses,
};

const MODULE: &str = "@celesta/react";
const BODY: &str = "\nexport const Scene = () => <Rect />;\n";

/// `text` after importing `names` from `@celesta/react`, and how to write
/// each.
fn ensure(text: &str, names: &[&str]) -> (String, Vec<ImportName>) {
    let (edit, written) = ensure_imports(text, MODULE, names).unwrap();
    let after = apply(text, edit.as_slice()).unwrap();
    assert!(parses(&after), "{after}");
    (after, written)
}

fn named(name: &str) -> ImportName {
    ImportName::Named(name.to_string())
}

fn clash(name: &str) -> ImportLock {
    ImportLock::Clash {
        name: name.to_string(),
    }
}

#[test]
fn adds_to_an_existing_named_import_keeping_it_sorted() {
    let text = format!("import {{ Rect, Text }} from '@celesta/react';\n{BODY}");
    assert_eq!(
        ensure(&text, &["Sequence"]),
        (
            format!("import {{ Rect, Sequence, Text }} from '@celesta/react';\n{BODY}"),
            vec![named("Sequence")]
        )
    );

    // After the last name; case is ignored.
    let text = format!("import {{ Rect, text }} from '@celesta/react';\n{BODY}");
    assert_eq!(
        ensure(&text, &["Video"]).0,
        format!("import {{ Rect, text, Video }} from '@celesta/react';\n{BODY}")
    );

    // A list on several lines keeps one name a line.
    let text = format!("import {{\n  Rect,\n  Text,\n}} from '@celesta/react';\n{BODY}");
    assert_eq!(
        ensure(&text, &["Sequence"]).0,
        format!("import {{\n  Rect,\n  Sequence,\n  Text,\n}} from '@celesta/react';\n{BODY}")
    );

    // Beside a default import.
    let text = format!("import Celesta, {{ Rect }} from '@celesta/react';\n{BODY}");
    assert_eq!(
        ensure(&text, &["Sequence"]).0,
        format!("import Celesta, {{ Rect, Sequence }} from '@celesta/react';\n{BODY}")
    );
}

#[test]
fn adds_two_new_names_to_a_sorted_import() {
    let text = format!("import {{ Rect, Text }} from '@celesta/react';\n{BODY}");
    let (edit, written) = ensure_imports(&text, MODULE, &["Sequence", "Circle"]).unwrap();
    assert!(matches!(edit, Some(Edit::Splice { .. })));
    assert_eq!(written, vec![named("Sequence"), named("Circle")]);
    assert_eq!(
        ensure(&text, &["Sequence", "Circle"]).0,
        format!("import {{ Circle, Rect, Sequence, Text }} from '@celesta/react';\n{BODY}")
    );
}

#[test]
fn adds_to_an_unsorted_import_at_the_end() {
    let text = format!("import {{ Text, Rect }} from '@celesta/react';\n{BODY}");
    assert_eq!(
        ensure(&text, &["Sequence"]),
        (
            format!("import {{ Text, Rect, Sequence }} from '@celesta/react';\n{BODY}"),
            vec![named("Sequence")]
        )
    );
}

#[test]
fn adds_a_new_import_after_the_last() {
    let text = format!(
        "import {{ useState }} from \"react\";\nimport {{ Circle }} from \"@celesta/shapes\";\n{BODY}"
    );
    assert_eq!(
        ensure(&text, &["Sequence"]),
        (
            format!(
                "import {{ useState }} from \"react\";\nimport {{ Circle }} from \"@celesta/shapes\";\nimport {{ Sequence }} from \"@celesta/react\";\n{BODY}"
            ),
            vec![named("Sequence")]
        )
    );

    // Single quotes, no semicolon.
    let text = format!("import {{ Circle }} from '@celesta/shapes'\n{BODY}");
    assert_eq!(
        ensure(&text, &["Sequence"]).0,
        format!(
            "import {{ Circle }} from '@celesta/shapes'\nimport {{ Sequence }} from '@celesta/react'\n{BODY}"
        )
    );

    // No import at all: first, after the directives.
    assert_eq!(
        ensure(BODY, &["Sequence"]).0,
        format!("import {{ Sequence }} from \"@celesta/react\";\n{BODY}")
    );
    let text = format!("'use client';\n{BODY}");
    assert_eq!(
        ensure(&text, &["Sequence"]).0,
        format!("'use client';\nimport {{ Sequence }} from \"@celesta/react\";\n{BODY}")
    );

    // A side-effect import is not extended.
    let text = format!("import '@celesta/react';\n{BODY}");
    assert_eq!(
        ensure(&text, &["Sequence"]).0,
        format!("import '@celesta/react';\nimport {{ Sequence }} from '@celesta/react';\n{BODY}")
    );
}

#[test]
fn adds_two_new_names_on_one_new_line() {
    let text = format!("import {{ useState }} from 'react';\n{BODY}");
    assert_eq!(
        ensure(&text, &["Sequence", "Circle"]),
        (
            format!(
                "import {{ useState }} from 'react';\nimport {{ Circle, Sequence }} from '@celesta/react';\n{BODY}"
            ),
            vec![named("Sequence"), named("Circle")]
        )
    );
}

#[test]
fn already_imported_needs_nothing() {
    let text = format!("import {{ Rect, Sequence }} from '@celesta/react';\n{BODY}");
    assert_eq!(
        ensure_import(&text, MODULE, "Sequence"),
        Ok((None, named("Sequence")))
    );
    // Only the missing name is added.
    assert_eq!(
        ensure(&text, &["Sequence", "Text"]),
        (
            format!("import {{ Rect, Sequence, Text }} from '@celesta/react';\n{BODY}"),
            vec![named("Sequence"), named("Text")]
        )
    );
}

#[test]
fn an_aliased_import_writes_the_alias() {
    let text = format!("import {{ Rect, Sequence as Seq }} from '@celesta/react';\n{BODY}");
    assert_eq!(
        ensure_import(&text, MODULE, "Sequence"),
        Ok((None, named("Seq")))
    );
}

#[test]
fn a_namespace_import_writes_a_member() {
    let text = format!("import * as C from '@celesta/react';\n{BODY}");
    assert_eq!(
        ensure_import(&text, MODULE, "Sequence"),
        Ok((None, ImportName::Namespace("C.Sequence".to_string())))
    );
}

#[test]
fn a_type_only_import_is_never_extended() {
    let text = format!("import type {{ SceneEntry }} from '@celesta/react';\n{BODY}");
    assert_eq!(
        ensure(&text, &["Sequence"]).0,
        format!(
            "import type {{ SceneEntry }} from '@celesta/react';\nimport {{ Sequence }} from '@celesta/react';\n{BODY}"
        )
    );

    for import in [
        "import type { Sequence } from '@celesta/react';",
        "import { Rect, type Sequence } from '@celesta/react';",
    ] {
        let text = format!("{import}\n{BODY}");
        assert_eq!(
            ensure_imports(&text, MODULE, &["Sequence"]),
            Err(clash("Sequence")),
            "{import}"
        );
    }
}

#[test]
fn a_clashing_local_binding_locks() {
    for text in [
        format!("import {{ Rect }} from '@celesta/react';\nconst Sequence = 1;\n{BODY}"),
        format!("import {{ Sequence }} from './mine';\n{BODY}"),
        // The import is shadowed somewhere.
        format!(
            "import {{ Sequence }} from '@celesta/react';\nfunction f(Sequence) {{ return Sequence; }}\n{BODY}"
        ),
    ] {
        assert_eq!(
            ensure_imports(&text, MODULE, &["Sequence"]),
            Err(clash("Sequence")),
            "{text}"
        );
    }

    // One clashing name of two: no edit at all.
    let text = format!("import {{ Rect }} from '@celesta/react';\nconst Sequence = 1;\n{BODY}");
    assert_eq!(
        ensure_imports(&text, MODULE, &["Circle", "Sequence"]),
        Err(clash("Sequence"))
    );
}
