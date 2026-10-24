use celesta_source_edit::{
    ElementRef, LockReason, element_at, text_style_path, written_attributes,
};

/// The element whose `<` is the first `needle` in `text` (ASCII only).
fn element(text: &str, needle: &str) -> ElementRef {
    let offset = text.find(needle).unwrap();
    let before = &text[..offset];
    let line = before.matches('\n').count() as u32 + 1;
    let column = (offset - before.rfind('\n').map_or(0, |i| i + 1)) as u32 + 1;
    element_at(text, line, column).unwrap()
}

/// A component `Scene` returning `jsx`.
fn scene(jsx: &str) -> String {
    format!("export function Scene() {{\n  return {jsx};\n}}\n")
}

/// The path the `Text` row `shorthand` edits on the first `<Text` of `jsx`.
fn path(jsx: &str, shorthand: &str, style_key: &str) -> Result<String, LockReason> {
    let text = scene(jsx);
    text_style_path(&text, &element(&text, "<Text"), shorthand, style_key)
}

#[test]
fn shorthand_without_style() {
    assert_eq!(
        path("<Text fontSize={48}>A</Text>", "fontSize", "fontSize"),
        Ok("fontSize".to_string())
    );
}

#[test]
fn style_literal_sets_the_key() {
    assert_eq!(
        path(
            "<Text style={{ fontSize: 48 }}>A</Text>",
            "fontSize",
            "fontSize"
        ),
        Ok("style.fontSize".to_string())
    );
    assert_eq!(
        path("<Text style={{ fill: '#fff' }}>A</Text>", "color", "fill"),
        Ok("style.fill".to_string())
    );
}

#[test]
fn style_literal_without_the_key() {
    assert_eq!(
        path(
            "<Text style={{ fontWeight: 700 }} fontSize={48}>A</Text>",
            "fontSize",
            "fontSize"
        ),
        Ok("fontSize".to_string())
    );
}

#[test]
fn style_not_a_literal_locks() {
    assert_eq!(
        path("<Text style={mono(20)}>A</Text>", "fontSize", "fontSize"),
        Err(LockReason::StyleMaySet)
    );
    assert_eq!(
        path(
            "<Text style={{ ...base, fontWeight: 700 }}>A</Text>",
            "fontSize",
            "fontSize"
        ),
        Err(LockReason::StyleMaySet)
    );
}

#[test]
fn an_element_spread_may_set_style() {
    // No `style` after the spread: it may carry one, which wins.
    assert_eq!(
        path(
            "<Text {...rest} fontSize={48}>A</Text>",
            "fontSize",
            "fontSize"
        ),
        Err(LockReason::StyleMaySet)
    );
    assert_eq!(
        path(
            "<Text style={{ fontWeight: 700 }} {...rest}>A</Text>",
            "fontSize",
            "fontSize"
        ),
        Err(LockReason::StyleMaySet)
    );
    // A `style` after the spread follows the usual rules.
    assert_eq!(
        path(
            "<Text {...rest} style={{ fontSize: 48 }}>A</Text>",
            "fontSize",
            "fontSize"
        ),
        Ok("style.fontSize".to_string())
    );
}

#[test]
fn an_object_spread_before_the_key_does_not_lock() {
    assert_eq!(
        path(
            "<Text style={{ ...base, fontSize: 48 }}>A</Text>",
            "fontSize",
            "fontSize"
        ),
        Ok("style.fontSize".to_string())
    );
    assert_eq!(
        path(
            "<Text style={{ fontSize: 48, ...base }}>A</Text>",
            "fontSize",
            "fontSize"
        ),
        Err(LockReason::StyleMaySet)
    );
}

#[test]
fn quoted_style_keys_count() {
    assert_eq!(
        path(
            "<Text style={{ 'fontSize': 48 }}>A</Text>",
            "fontSize",
            "fontSize"
        ),
        Ok("style.fontSize".to_string())
    );
}

#[test]
fn style_path_errors_as_resolve() {
    let text = scene("<Text fontSize={48}>A</Text>");
    let mut missing = element(&text, "<Text");
    missing.start += 1;
    assert_eq!(
        text_style_path(&text, &missing, "fontSize", "fontSize"),
        Err(LockReason::ElementNotFound)
    );
    let broken = format!("{text}<");
    assert_eq!(
        text_style_path(&broken, &element(&text, "<Text"), "fontSize", "fontSize"),
        Err(LockReason::SyntaxError)
    );
}

#[test]
fn attributes_in_source_order() {
    let text = scene("<PriceTag key=\"a\" x={40} {...rest} label=\"Sale\" ref={r} x={41} />");
    assert_eq!(
        written_attributes(&text, &element(&text, "<PriceTag")),
        vec!["label".to_string(), "x".to_string()]
    );
}

#[test]
fn attributes_of_nothing() {
    let text = scene("<PriceTag x={40} />");
    let mut missing = element(&text, "<PriceTag");
    missing.start += 1;
    assert_eq!(written_attributes(&text, &missing), Vec::<String>::new());
    let broken = format!("{text}<");
    assert_eq!(
        written_attributes(&broken, &element(&text, "<PriceTag")),
        Vec::<String>::new()
    );
}
