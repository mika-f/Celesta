use celesta_composition::Time;
use celesta_evaluator::Evaluator;
use celesta_project::{Project, TimelineContent};
use celesta_renderer::CpuRenderer;

#[test]
fn renders_an_evaluated_project_scene() {
    let mut project =
        Project::from_json(include_str!("../../../examples/voiceroid.celesta.json")).unwrap();
    project.characters.get_mut("akane").unwrap().portrait = None;
    let TimelineContent::Dialogue { expression, .. } = &mut project.tracks[0].items[0].content
    else {
        panic!("example contains a dialogue item");
    };
    *expression = None;
    let scene = Evaluator::new(&project)
        .unwrap()
        .scene_at(Time::new(6, 1))
        .unwrap();
    let mut renderer = CpuRenderer::default();
    let frame = renderer.render(&scene).unwrap();

    assert_eq!(frame.width(), 1920);
    assert_eq!(frame.height(), 1080);
    assert!(
        frame
            .pixels()
            .chunks_exact(4)
            .any(|pixel| pixel != [20, 22, 28, 255])
    );
}
