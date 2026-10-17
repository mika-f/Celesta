use crate::common::{call, client, entry, is_error, runtime_built, structured, text};
use base64::Engine;
use rmcp::model::CallToolResult;
use serde_json::json;

const PNG_SIGNATURE: &[u8] = b"\x89PNG\r\n\x1a\n";

fn scene(frames: u32) -> String {
    format!(
        r##"
import {{ Composition, Rect, useCurrentFrame }} from '@celesta/react';
function Box() {{
  return <Rect x={{useCurrentFrame() * 4}} width={{32}} height={{32}} fill="#ff3366" />;
}}
export default function Root() {{
  return <Composition width={{160}} height={{90}} fps={{30}} durationInFrames={{{frames}}}>
    <Rect width={{160}} height={{90}} fill="#101820" /><Box />
  </Composition>;
}}
"##
    )
}

/// A 16x16, 4 fps, one-second JSON project in `dir`.
fn project(dir: &std::path::Path, width: u32) -> std::path::PathBuf {
    let path = dir.join("project.celesta.json");
    let project = json!({
        "version": 0,
        "settings": {
            "width": width, "height": 16,
            "frameRate": { "numerator": 4, "denominator": 1 },
            "duration": { "value": 1, "timescale": 1 },
            "sampleRate": 48000
        },
        "assets": {}, "characters": {}, "tracks": [], "properties": {}
    });
    std::fs::write(&path, project.to_string()).unwrap();
    path
}

fn images(result: &CallToolResult) -> Vec<Vec<u8>> {
    typed_images(result)
        .into_iter()
        .map(|(_, bytes)| bytes)
        .collect()
}

/// Each image's MIME type and bytes.
fn typed_images(result: &CallToolResult) -> Vec<(String, Vec<u8>)> {
    result
        .content
        .iter()
        .filter_map(|block| block.as_image())
        .map(|image| {
            let bytes = base64::engine::general_purpose::STANDARD
                .decode(&image.data)
                .unwrap();
            (image.mime_type.clone(), bytes)
        })
        .collect()
}

fn is_jpeg(bytes: &[u8]) -> bool {
    bytes.starts_with(&[0xff, 0xd8, 0xff])
}

fn is_webp(bytes: &[u8]) -> bool {
    bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WEBP")
}

#[tokio::test]
async fn render_frames_returns_pngs() {
    if !runtime_built() {
        return;
    }
    let (_dir, entry) = entry(&scene(10));
    let (client, _) = client().await;
    let result = call(
        &client,
        "render_frames",
        json!({ "source": entry, "frames": [0, -1] }),
    )
    .await;
    assert!(!is_error(&result), "{}", text(&result));
    let images = images(&result);
    assert_eq!(images.len(), 2);
    assert!(images.iter().all(|png| png.starts_with(PNG_SIGNATURE)));
    let outputs = structured(&result)["outputs"].as_array().unwrap();
    let frames: Vec<u64> = outputs
        .iter()
        .map(|o| o["frame"].as_u64().unwrap())
        .collect();
    assert_eq!(frames, [0, 9]);
    for output in outputs {
        assert!(std::path::Path::new(output["path"].as_str().unwrap()).is_absolute());
    }
}

#[tokio::test]
async fn render_frames_inlines_at_most_six() {
    if !runtime_built() {
        return;
    }
    let (_dir, entry) = entry(&scene(10));
    let (client, _) = client().await;
    let result = call(
        &client,
        "render_frames",
        json!({ "source": entry, "every": 1 }),
    )
    .await;
    assert!(!is_error(&result), "{}", text(&result));
    assert_eq!(images(&result).len(), 6);
    assert_eq!(structured(&result)["outputs"].as_array().unwrap().len(), 10);
}

#[tokio::test]
async fn render_frames_twice_does_not_collide() {
    if !runtime_built() {
        return;
    }
    let (_dir, entry) = entry(&scene(10));
    let (client, _) = client().await;
    for _ in 0..2 {
        let result = call(
            &client,
            "render_frames",
            json!({ "source": entry, "frames": [0] }),
        )
        .await;
        assert!(!is_error(&result), "{}", text(&result));
    }
}

#[tokio::test]
async fn render_frames_resolves_a_relative_source() {
    if !runtime_built() {
        return;
    }
    // The server's working directory is the test's: crates/mcp.
    let dir = tempfile::tempdir_in(env!("CARGO_MANIFEST_DIR")).unwrap();
    project(dir.path(), 16);
    let relative = format!(
        "{}/project.celesta.json",
        dir.path().file_name().unwrap().to_str().unwrap()
    );
    let (client, _) = client().await;
    let result = call(
        &client,
        "render_frames",
        json!({ "source": relative, "frames": [0] }),
    )
    .await;
    assert!(!is_error(&result), "{}", text(&result));
    assert!(std::path::Path::new(structured(&result)["source"].as_str().unwrap()).is_absolute());
}

#[tokio::test]
async fn output_dir_refuses_existing_files_unless_overwrite() {
    if !runtime_built() {
        return;
    }
    let (_dir, entry) = entry(&scene(10));
    let out = tempfile::tempdir().unwrap();
    let (client, _) = client().await;
    let args = json!({ "source": entry, "frames": [0], "outputDir": out.path() });
    assert!(!is_error(
        &call(&client, "render_frames", args.clone()).await
    ));
    let again = call(&client, "render_frames", args.clone()).await;
    assert!(is_error(&again));
    let error = &structured(&again)["error"];
    assert_eq!(error["code"], "output_exists");
    let hint = error["hint"].as_str().unwrap();
    assert!(hint.contains("overwrite") && !hint.contains("--"), "{hint}");
    let mut overwrite = args;
    overwrite["overwrite"] = json!(true);
    assert!(!is_error(&call(&client, "render_frames", overwrite).await));
}

#[tokio::test]
async fn render_frames_validates_json_projects() {
    if !runtime_built() {
        return;
    }
    let (client, _) = client().await;
    let dir = tempfile::tempdir().unwrap();
    let valid = project(dir.path(), 16);
    let ok = call(
        &client,
        "render_frames",
        json!({ "source": valid, "frames": [0, -1] }),
    )
    .await;
    assert!(!is_error(&ok), "{}", text(&ok));
    assert_eq!(images(&ok).len(), 2);

    let broken = project(dir.path(), 0);
    let result = call(
        &client,
        "render_frames",
        json!({ "source": broken, "frames": [0] }),
    )
    .await;
    assert!(is_error(&result));
    let error = &structured(&result)["error"];
    assert_eq!(error["code"], "project_validation");
    assert_eq!(error["issues"][0]["path"], "settings.width");
}

#[tokio::test]
async fn react_arguments_on_json_are_usage_errors() {
    let (client, _) = client().await;
    let result = call(
        &client,
        "render_frames",
        json!({ "source": "a.celesta.json", "frames": [0], "props": { "a": 1 } }),
    )
    .await;
    assert!(is_error(&result));
    assert_eq!(structured(&result)["error"]["code"], "usage");
}

#[tokio::test]
async fn render_frames_reports_invalid_properties() {
    if !runtime_built() {
        return;
    }
    let (_dir, entry) = entry(
        r#"
import { Composition, defineProjectProperties } from '@celesta/react';
defineProjectProperties({ n: { type: 'number', defaultValue: 1 } });
export default function Root() {
  return <Composition width={160} height={90} fps={30} durationInFrames={10} />;
}
"#,
    );
    let (client, _) = client().await;
    let result = call(
        &client,
        "render_frames",
        json!({ "source": entry, "frames": [0], "props": { "n": "x" } }),
    )
    .await;
    assert!(is_error(&result), "{}", text(&result));
    assert_eq!(structured(&result)["error"]["code"], "invalid_properties");
    assert_eq!(structured(&result)["error"]["issues"][0]["path"], "n");
}

#[tokio::test]
async fn contact_sheet_returns_one_image() {
    if !runtime_built() {
        return;
    }
    let (_dir, entry) = entry(&scene(20));
    let (client, _) = client().await;
    let result = call(
        &client,
        "contact_sheet",
        json!({ "source": entry, "every": 5 }),
    )
    .await;
    assert!(!is_error(&result), "{}", text(&result));
    let images = typed_images(&result);
    assert_eq!(images.len(), 1);
    assert_eq!(images[0].0, "image/png");
    assert!(images[0].1.starts_with(PNG_SIGNATURE));
    assert_eq!(
        structured(&result)["outputs"][0]["frames"],
        json!([0, 5, 10, 15, 19])
    );
}

#[tokio::test]
async fn render_frames_writes_and_returns_jpeg_and_webp() {
    if !runtime_built() {
        return;
    }
    let (_dir, entry) = entry(&scene(10));
    let (client, _) = client().await;
    for (format, extension, matches) in [
        ("jpeg", ".jpeg", is_jpeg as fn(&[u8]) -> bool),
        ("webp", ".webp", is_webp),
    ] {
        let result = call(
            &client,
            "render_frames",
            json!({ "source": entry, "frames": [0, -1], "format": format, "quality": 60 }),
        )
        .await;
        assert!(!is_error(&result), "{}", text(&result));
        let images = typed_images(&result);
        assert_eq!(images.len(), 2);
        for (mime_type, bytes) in &images {
            assert_eq!(mime_type, &format!("image/{format}"));
            assert!(matches(bytes), "{format}");
        }
        for output in structured(&result)["outputs"].as_array().unwrap() {
            let path = std::path::Path::new(output["path"].as_str().unwrap());
            assert!(
                path.to_str().unwrap().ends_with(extension),
                "{}",
                path.display()
            );
            assert!(matches(&std::fs::read(path).unwrap()), "{}", path.display());
        }
    }
}

#[tokio::test]
async fn contact_sheet_writes_the_output_extension_format() {
    if !runtime_built() {
        return;
    }
    let (_dir, entry) = entry(&scene(20));
    let out = tempfile::tempdir().unwrap();
    let (client, _) = client().await;
    let sheet = out.path().join("sheet.webp");
    // Five 160-pixel tiles stay under the inline limit, so the image comes
    // back as written rather than as a downscaled PNG.
    let result = call(
        &client,
        "contact_sheet",
        json!({ "source": entry, "every": 5, "tileWidth": 160, "output": sheet, "quality": 80 }),
    )
    .await;
    assert!(!is_error(&result), "{}", text(&result));
    let images = typed_images(&result);
    assert_eq!(images.len(), 1);
    assert_eq!(images[0].0, "image/webp");
    assert!(is_webp(&images[0].1));
    assert!(is_webp(&std::fs::read(&sheet).unwrap()));

    let result = call(
        &client,
        "contact_sheet",
        json!({ "source": entry, "every": 5, "tileWidth": 160, "format": "jpeg" }),
    )
    .await;
    assert!(!is_error(&result), "{}", text(&result));
    let images = typed_images(&result);
    assert_eq!(images[0].0, "image/jpeg");
    assert!(is_jpeg(&images[0].1));
    let path = structured(&result)["outputs"][0]["path"]
        .as_str()
        .unwrap()
        .to_owned();
    assert!(path.ends_with("sheet.jpeg"), "{path}");
}

#[tokio::test]
async fn still_quality_is_checked_before_rendering() {
    let (client, _) = client().await;
    for (tool, args) in [
        (
            "render_frames",
            json!({ "source": "a.tsx", "frames": [0], "quality": 80 }),
        ),
        (
            "render_frames",
            json!({ "source": "a.tsx", "frames": [0], "format": "jpeg", "quality": 0 }),
        ),
        (
            "contact_sheet",
            json!({ "source": "a.tsx", "every": 5, "output": "a.png", "quality": 80 }),
        ),
        (
            "contact_sheet",
            json!({ "source": "a.tsx", "every": 5, "output": "a.gif" }),
        ),
    ] {
        let result = call(&client, tool, args.clone()).await;
        assert!(is_error(&result), "{args}");
        assert_eq!(structured(&result)["error"]["code"], "usage", "{args}");
    }
}
