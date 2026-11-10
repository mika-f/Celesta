//! The tests' one gate on Media Foundation (Ruling 17), for the unit tests
//! and the integration tests alike, so it is public but hidden.
//!
//! A test that needs Media Foundation, or one of the plugin's components,
//! skips with the reason `describe` gives when this system lacks it, unless
//! `CELESTA_REQUIRE_MEDIA_FOUNDATION=1` says this machine has them: then it
//! fails instead, as `CELESTA_REQUIRE_GPU=1` makes a missing GPU adapter
//! fail. What depends on the machine's hardware (a GPU's encoder refusing,
//! the system choosing Microsoft's software one, an encoder that does not
//! reorder) skips either way.

use crate::MfCodec;
use celesta_media::{ComponentDescription, MediaProvider};

/// The variable that makes a missing Media Foundation, or a missing
/// component, fail a test rather than skip it.
pub const REQUIRE: &str = "CELESTA_REQUIRE_MEDIA_FOUNDATION";

/// Whether `CELESTA_REQUIRE_MEDIA_FOUNDATION=1` says this machine has
/// Media Foundation and every component of the plugin.
pub fn required() -> bool {
    std::env::var(REQUIRE).as_deref() == Ok("1")
}

/// Prints that `test` skips for `reason`, or fails it when [`required`].
pub fn skip(test: &str, reason: &str) {
    assert!(
        !required(),
        "{test} needs what this system lacks ({REQUIRE}=1): {reason}"
    );
    println!("skipping {test}: {reason}");
}

/// Whether `codec` describes every one of `components` (ids: the decoder
/// `mediafoundation`, `h264`, `hevc`, `aac`) as available; when one is not,
/// [`skip`]s `test` with its reason. Media Foundation has started exactly
/// when the decoder is available.
pub fn available(codec: &mut MfCodec, components: &[&str], test: &str) -> bool {
    let described = codec.describe().expect("describe answers");
    match missing(&described, components) {
        Some(reason) => {
            skip(test, &reason);
            false
        }
        None => true,
    }
}

/// Why the first of `components` that `described` has unavailable is, if
/// one is.
fn missing(described: &[ComponentDescription], components: &[&str]) -> Option<String> {
    components.iter().find_map(|id| {
        let component = described
            .iter()
            .find(|component| component.id == *id)
            .unwrap_or_else(|| panic!("{id} is not described"));
        (!component.available).then(|| {
            component
                .reason
                .clone()
                .unwrap_or_else(|| "no reason".to_owned())
        })
    })
}

/// Media Foundation started, for a unit test that calls it directly, or
/// None when it does not start and `test` [`skip`]s.
#[cfg(test)]
pub(crate) fn started(test: &str) -> Option<crate::mf::startup::Started> {
    match crate::mf::startup::Started::new() {
        Ok(started) => Some(started),
        Err(error) => {
            skip(test, &crate::components::not_started_reason(&error));
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::{Encoders, VideoCodec, describe_from, not_started_reason};

    #[test]
    fn a_missing_component_skips_with_describes_reason() {
        // `MFStartup` failed: every component, the decoder first, says why.
        let error = "MFStartup failed: 0x80004005".to_owned();
        let none = Encoders {
            h264: false,
            hevc: false,
            aac: false,
        };
        let described = describe_from(Err(error.clone()), &none);
        for id in ["mediafoundation", "h264", "aac"] {
            assert_eq!(
                missing(&described, &[id]),
                Some(not_started_reason(&error)),
                "{id}"
            );
        }
        // Started, without HEVC: only a test that needs it skips.
        let described = describe_from(
            Ok(()),
            &Encoders {
                h264: true,
                hevc: false,
                aac: true,
            },
        );
        assert_eq!(
            missing(&described, &["mediafoundation", "h264", "aac"]),
            None
        );
        assert_eq!(
            missing(&described, &["h264", "hevc"]),
            Some(VideoCodec::Hevc.missing_reason().to_owned())
        );
    }
}
