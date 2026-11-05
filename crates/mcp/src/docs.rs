//! The `celesta` skill's Markdown, embedded so the docs an agent reads
//! match the server it runs.

use std::sync::OnceLock;

pub struct Topic {
    pub name: &'static str,
    pub description: &'static str,
    pub markdown: &'static str,
}

macro_rules! reference {
    ($name:literal, $description:literal) => {
        Topic {
            name: $name,
            description: $description,
            markdown: include_str!(concat!(
                env!("CELESTA_SKILL_DIR"),
                "/references/",
                $name,
                ".md"
            )),
        }
    };
}

const SKILL: &str = include_str!(concat!(env!("CELESTA_SKILL_DIR"), "/SKILL.md"));

pub static TOPICS: &[Topic] = &[
    Topic {
        name: "overview",
        description: "Formats, the rules that are easy to get wrong, the workflow, and the package list",
        markdown: SKILL,
    },
    reference!(
        "setup",
        "Find the tools, start a project (templates), npm packages, type-checking, the MCP server"
    ),
    reference!(
        "react-core",
        "Entry files and prepare(), layers, shapes, Group, Image/Video/Audio, fonts, layout helpers, safe areas"
    ),
    reference!(
        "animation",
        "Frame hooks, interpolate, easing, spring, Sequence/Series/TransitionSeries, Stagger, beats, cues, Camera"
    ),
    reference!(
        "text",
        "Text and TextStyle, fonts and fallback, language, line breaking, measuring and fitting text"
    ),
    reference!(
        "subtitles",
        "SRT/WebVTT subtitle files: reading them into cues, Subtitles, writing them on export"
    ),
    reference!(
        "code",
        "Syntax-highlighted and typed code (@celesta.studio/code)"
    ),
    reference!(
        "helpers",
        "The API to use before writing a helper: clamp, eased progress, random, timecodes, text widths, cues"
    ),
    reference!(
        "math",
        "Seeded random, noise, and math helpers (@celesta.studio/math)"
    ),
    reference!("loops", "Seamless loops for Shorts and TikTok"),
    reference!("shader", "Custom WGSL filters (@celesta.studio/shader)"),
    reference!(
        "dialogue",
        "Characters, subtitles, voices, lip sync, PSD portraits, blinking"
    ),
    reference!("project-json", "The .celesta.json project format"),
    reference!(
        "project-data",
        "Project properties, props.json, ProjectTimeline, registerComponent"
    ),
    reference!(
        "verify",
        "Checking frames, contact sheets, JSON projects, and audio"
    ),
    reference!(
        "export",
        "Video export options (MP4, MOV, WebM), JSON results and error codes, exporting without a GPU"
    ),
    reference!("performance", "What is slow to render and how to find it"),
    reference!(
        "plugins",
        "Content plugins: @celesta-plugin imports, where plugins are found, their errors"
    ),
    reference!(
        "errors",
        "Error messages and warnings, and what to do about them"
    ),
];

/// The topic's Markdown, or the table of contents without one.
pub fn read(topic: Option<&str>) -> Result<String, String> {
    let Some(name) = topic else {
        let mut index =
            String::from("# Celesta documentation\n\nCall read_docs with one of these topics:\n\n");
        for topic in TOPICS {
            index.push_str(&format!("- `{}` — {}\n", topic.name, topic.description));
        }
        return Ok(index);
    };
    match TOPICS.iter().find(|topic| topic.name == name) {
        Some(topic) if topic.name == "overview" => Ok(overview().to_owned()),
        Some(topic) => Ok(topic.markdown.to_owned()),
        None => Err(format!(
            "unknown topic {name:?}; topics are {}",
            TOPICS
                .iter()
                .map(|topic| topic.name)
                .collect::<Vec<_>>()
                .join(", ")
        )),
    }
}

/// `SKILL.md` without its front matter.
fn overview() -> &'static str {
    static OVERVIEW: OnceLock<&'static str> = OnceLock::new();
    OVERVIEW.get_or_init(|| {
        SKILL
            .strip_prefix("---")
            .and_then(|rest| rest.split_once("\n---"))
            .map_or(SKILL, |(_, body)| body.trim_start())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_reference_is_a_topic() {
        let dir = concat!(env!("CELESTA_SKILL_DIR"), "/references");
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.extension().is_some_and(|extension| extension == "md") {
                let stem = path.file_stem().unwrap().to_str().unwrap();
                assert!(
                    TOPICS.iter().any(|topic| topic.name == stem),
                    "{stem}.md is not embedded in read_docs"
                );
            }
        }
        assert!(TOPICS.iter().any(|topic| topic.name == "overview"));
    }

    #[test]
    fn overview_has_no_front_matter() {
        let overview = read(Some("overview")).unwrap();
        assert!(!overview.starts_with("---"), "{}", &overview[..40]);
        assert!(overview.starts_with("# Celesta"));
    }

    #[test]
    fn unknown_topics_name_the_valid_ones() {
        let error = read(Some("nope")).unwrap_err();
        assert!(error.contains("animation"), "{error}");
    }
}
