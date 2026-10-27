use celesta_source_edit::{ApplyError, Edit, EditValue, LiteralForm, apply};

#[test]
fn splice_replaces_verbatim() {
    let text = "<Rect width={200} />";
    let start = text.find("200").unwrap() as u32;
    let splice = Edit::Splice {
        start,
        end: start + 3,
        text: "rectWidth".into(),
    };
    assert_eq!(
        apply(text, std::slice::from_ref(&splice)).unwrap(),
        "<Rect width={rectWidth} />"
    );

    // An empty span inserts the text as written.
    let insert = Edit::Splice {
        start: start - 1,
        end: start - 1,
        text: "/* w */".into(),
    };
    assert_eq!(
        apply(text, &[insert]).unwrap(),
        "<Rect width=/* w */{200} />"
    );

    let replace = Edit::Replace {
        start: start + 1,
        end: start + 2,
        value: EditValue::Number(5.0),
        form: LiteralForm::Number,
    };
    assert_eq!(apply(text, &[splice, replace]), Err(ApplyError::Overlap));
}
