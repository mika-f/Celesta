//! The quality checks of a React entry through the command line; the JSON
//! project checks are in the `integration` target (`tests/integration/quality.rs`).

use crate::common::{quality_exporter, root};

#[test]
fn a_react_check_warns_about_rough_scenes_and_final_refuses_them() {
    crate::runtime();
    let root = root();
    // In the CLI package, where `@celesta.studio/react` resolves.
    let dir = tempfile::tempdir_in(root.join("packages/cli")).unwrap();
    let entry = dir.path().join("film.tsx");
    std::fs::write(
        &entry,
        r##"
import { Composition, Rect, Text } from '@celesta.studio/react';
const chapters = [
  { id: 'intro', from: 0, durationInFrames: 30 },
  { id: 'card', name: 'The card', from: 30, durationInFrames: 30, status: 'rough' },
];
export default function Root() {
  return <Composition width={160} height={90} fps={30} durationInFrames={60} chapters={chapters}>
    <Rect width={160} height={90} fill="#101018" />
    <Text x={80} y={45} style={{ fontSize: 24 }}>Title</Text>
  </Composition>;
}
"##,
    )
    .unwrap();
    let entry = entry.to_str().unwrap();
    let Some((code, report)) = quality_exporter(&["--react", "--check-quality", entry]) else {
        return;
    };
    assert_eq!(report["status"], "ok", "{report}");
    // A rough scene is something found: the check fails.
    assert_eq!(code, Some(1), "{report}");
    let coded: Vec<&str> = report["codedWarnings"]
        .as_array()
        .unwrap_or_else(|| panic!("{report}"))
        .iter()
        .map(|warning| warning["code"].as_str().unwrap())
        .collect();
    assert!(coded.contains(&"rough_scenes"), "{report}");
    // --final refuses the span, as it would refuse to export it.
    let (code, report) =
        quality_exporter(&["--react", "--check-quality", "--final", entry]).unwrap();
    assert_eq!(code, Some(1), "{report}");
    assert_eq!(report["error"]["code"], "rough_scenes", "{report}");
}
