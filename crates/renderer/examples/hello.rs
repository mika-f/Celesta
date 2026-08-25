use std::error::Error;

use mikan_composition::{
    EvaluatedTransform, Layer, LayerContent, Point, Rational, Scene, TextAlign, TextStyle, Time,
};
use mikan_renderer::CpuRenderer;

fn main() -> Result<(), Box<dyn Error>> {
    let output = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "hello.png".to_owned());
    let scene = Scene {
        width: 1280,
        height: 720,
        frame_rate: Rational::new(60, 1),
        time: Time::ZERO,
        fonts: Vec::new(),
        layers: vec![Layer {
            id: "hello".to_owned(),
            transform: EvaluatedTransform {
                position: Point { x: 640.0, y: 360.0 },
                ..EvaluatedTransform::default()
            },
            opacity: 1.0,
            content: LayerContent::Text {
                text: "Mikan へようこそ".to_owned(),
                style: TextStyle {
                    font_size: Some(72.0),
                    align: Some(TextAlign::Center),
                    ..TextStyle::default()
                },
                max_width: None,
            },
        }],
    };

    let mut renderer = CpuRenderer::default();
    renderer.render(&scene)?.write_png(&output)?;
    println!("rendered {output}");
    Ok(())
}
