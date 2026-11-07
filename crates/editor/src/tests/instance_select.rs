//! Tests of picking a component instance from the canvas: the instance a
//! click selects (`instance_target`).

use super::*;
use crate::layer_select::instance_target;

/// A `<Rect>` `r` in `PriceTag`, whose root `<Group>` is `g`, in `Film`,
/// whose root `<Group>` is `f`: innermost first, as the runtime reports it.
fn film_chain() -> Vec<LayerSource> {
    vec![
        chain_entry("Rect", ElementKind::Primitive, Some("r")),
        chain_entry("Group", ElementKind::Primitive, Some("g")),
        chain_entry("PriceTag", ElementKind::Component, Some("g")),
        chain_entry("Group", ElementKind::Primitive, Some("f")),
        chain_entry("Film", ElementKind::Component, Some("f")),
    ]
}

#[test]
fn instance_click_picks_the_outermost_instance() {
    let hit = report_of(film_chain());
    assert_eq!(instance_target(&hit, None, None, false), Some("f".into()));
}

#[test]
fn instance_click_inside_the_context_picks_inside_it() {
    let hit = report_of(film_chain());
    // Another price tag, `h`, in the same `Film`, is selected.
    let selected = report_of(vec![
        chain_entry("Group", ElementKind::Primitive, Some("h")),
        chain_entry("PriceTag", ElementKind::Component, Some("h")),
        chain_entry("Group", ElementKind::Primitive, Some("f")),
        chain_entry("Film", ElementKind::Component, Some("f")),
    ]);
    assert_eq!(
        instance_target(&hit, Some(&selected), Some("h"), false),
        Some("g".into())
    );
}

#[test]
fn instance_click_outside_the_context_starts_over() {
    let hit = report_of(film_chain());
    // The selection sits in another film, `x`, which the hit is not in.
    let selected = report_of(vec![
        chain_entry("Group", ElementKind::Primitive, Some("h")),
        chain_entry("PriceTag", ElementKind::Component, Some("h")),
        chain_entry("Group", ElementKind::Primitive, Some("x")),
        chain_entry("Film", ElementKind::Component, Some("x")),
    ]);
    assert_eq!(
        instance_target(&hit, Some(&selected), Some("h"), false),
        Some("f".into())
    );
}

#[test]
fn instance_descend_picks_one_level_down() {
    let hit = report_of(film_chain());
    let film = report_of(film_chain()[3..].to_vec());
    assert_eq!(
        instance_target(&hit, Some(&film), Some("f"), true),
        Some("g".into())
    );
    // Inside the price tag no instance is left: the `<Rect>` itself.
    let tag = report_of(film_chain()[1..].to_vec());
    assert_eq!(instance_target(&hit, Some(&tag), Some("g"), true), None);
}

#[test]
fn instance_pick_skips_a_root_shared_with_the_context() {
    // `<Frame>{children}</Frame>` around `<Film>` draws `Film`'s root.
    let mut chain = film_chain();
    chain.push(chain_entry("Frame", ElementKind::Component, Some("f")));
    let hit = report_of(chain.clone());
    let film = report_of(chain[3..].to_vec());
    assert_eq!(
        instance_target(&hit, Some(&film), Some("f"), true),
        Some("g".into())
    );
    // From nothing selected, the outermost is the shared root, once.
    assert_eq!(instance_target(&hit, None, None, false), Some("f".into()));
}

#[test]
fn instance_pick_without_instances_keeps_the_hit() {
    let hit = report_of(vec![
        chain_entry("Rect", ElementKind::Primitive, Some("r")),
        chain_entry("Sequence", ElementKind::Wrapper, None),
        // A component that draws several layers has no root.
        chain_entry("Pair", ElementKind::Component, None),
    ]);
    assert_eq!(instance_target(&hit, None, None, false), None);
}

#[test]
fn instance_pick_of_a_component_drawing_the_hit_itself_keeps_it() {
    // `Mark` returns the `<Rect>` alone: its root is the hit.
    let hit = report_of(vec![
        chain_entry("Rect", ElementKind::Primitive, Some("m")),
        chain_entry("Mark", ElementKind::Component, Some("m")),
    ]);
    assert_eq!(instance_target(&hit, None, None, false), None);
}

#[test]
fn press_pick_follows_mode_and_modifier() {
    use crate::layer_select::{PickMode, Press, press_pick};
    assert_eq!(press_pick(PickMode::Layer, false, false), Press::Layer);
    assert_eq!(press_pick(PickMode::Layer, false, true), Press::Layer);
    // Cmd+click (Ctrl+click) picks the layer itself.
    assert_eq!(press_pick(PickMode::Instance, true, false), Press::Layer);
    assert_eq!(
        press_pick(PickMode::Instance, false, false),
        Press::Instance { descend: false }
    );
    assert_eq!(
        press_pick(PickMode::Instance, false, true),
        Press::Instance { descend: true }
    );
}

fn pending(descend: bool) -> crate::layer_select::PendingInstance {
    crate::layer_select::PendingInstance {
        hit: "r".into(),
        selected: None,
        selected_id: None,
        descend,
    }
}

#[test]
fn pending_instance_moves_once_found() {
    use crate::layer_select::pending_instance_fate;
    let found = SourceLookup::Found(Box::new(report_of(film_chain())));
    assert_eq!(
        pending_instance_fate(&pending(false), Some("r"), &found),
        Some("f".into())
    );
}

#[test]
fn pending_instance_drops_on_a_newer_selection() {
    use crate::layer_select::pending_instance_fate;
    let found = SourceLookup::Found(Box::new(report_of(film_chain())));
    assert_eq!(
        pending_instance_fate(&pending(false), Some("other"), &found),
        None
    );
    assert_eq!(pending_instance_fate(&pending(false), None, &found), None);
}

#[test]
fn pending_instance_drops_without_a_chain() {
    use crate::layer_select::pending_instance_fate;
    for lookup in [
        SourceLookup::NotInFrame,
        SourceLookup::NoPosition,
        SourceLookup::Failed("no runtime".into()),
        SourceLookup::Pending,
    ] {
        assert_eq!(
            pending_instance_fate(&pending(false), Some("r"), &lookup),
            None
        );
    }
}
