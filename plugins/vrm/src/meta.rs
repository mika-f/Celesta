//! The avatar's usage terms (`VRMC_vrm.meta`) as notices, so whoever
//! renders it sees what its author allows.
//!
//! One notice a line: who made it and its licence, then each term. A term
//! that likely forbids the video being made (the avatar only for its
//! author or for people licensed separately, no commercial use, a credit
//! required) is a `warning`; every other line is `info`. A term absent from
//! the file takes VRM 1.0's default. The plugin never refuses to draw for
//! the terms: whether a use is allowed is for the person to decide.

use celesta_plugin::{Notice, NoticeLevel};
use vrm_spec::vrmc_vrm_1_0::{
    AvatarPermissionType, CommercialUsageType, CreditNotationType, Meta, ModificationType,
};

/// The notices for `meta`, in the order the terms are listed above.
pub(crate) fn usage_notices(meta: &Meta) -> Vec<Notice> {
    let mut notices = Vec::new();
    let mut note = |level: NoticeLevel, message: String| notices.push(Notice { level, message });
    let info = NoticeLevel::Info;
    let warning = NoticeLevel::Warning;

    let authors = if meta.authors.is_empty() {
        "an unnamed author".to_owned()
    } else {
        meta.authors.join(", ")
    };
    let version = given(&meta.version).map_or(String::new(), |version| format!(" {version}"));
    let mut heading = format!(
        "VRM {:?}{version} by {authors}; licence {}",
        meta.name, meta.license_url
    );
    if let Some(other) = given(&meta.other_license_url) {
        heading.push_str(&format!("; other licence {other}"));
    }
    note(info, heading);
    if let Some(copyright) = given(&meta.copyright_information) {
        note(info, format!("Copyright: {copyright}"));
    }
    if let Some(contact) = given(&meta.contact_information) {
        note(info, format!("Contact: {contact}"));
    }
    if let Some(references) = meta.references.as_ref().filter(|r| !r.is_empty()) {
        note(info, format!("References: {}", references.join("; ")));
    }
    if let Some(licences) = given(&meta.third_party_licenses) {
        note(info, format!("Third-party licences: {licences}"));
    }

    match meta
        .avatar_permission
        .unwrap_or(AvatarPermissionType::OnlyAuthor)
    {
        AvatarPermissionType::Everyone => note(info, "Avatar use: anyone".into()),
        AvatarPermissionType::OnlySeparatelyLicensedPerson => note(
            warning,
            "Avatar use: only people the author licenses separately".into(),
        ),
        AvatarPermissionType::OnlyAuthor => note(warning, "Avatar use: only the author".into()),
    }
    match meta
        .commercial_usage
        .unwrap_or(CommercialUsageType::PersonalNonProfit)
    {
        CommercialUsageType::PersonalNonProfit => note(
            warning,
            "Commercial use: not allowed (personal, non-profit only)".into(),
        ),
        CommercialUsageType::PersonalProfit => note(
            info,
            "Commercial use: individuals only, not corporations".into(),
        ),
        CommercialUsageType::Corporation => note(info, "Commercial use: allowed".into()),
    }
    match meta.credit_notation.unwrap_or(CreditNotationType::Required) {
        CreditNotationType::Required => note(
            warning,
            format!("Credit: required; credit {authors} where the video is published"),
        ),
        CreditNotationType::Unnecessary => note(info, "Credit: not required".into()),
    }
    let allowed = |allowed: Option<bool>| {
        if allowed.unwrap_or(false) {
            "allowed"
        } else {
            "not allowed"
        }
    };
    for (what, flag) in [
        ("Excessively violent", meta.allow_excessively_violent_usage),
        ("Excessively sexual", meta.allow_excessively_sexual_usage),
        (
            "Political or religious",
            meta.allow_political_or_religious_usage,
        ),
        ("Antisocial or hateful", meta.allow_antisocial_or_hate_usage),
    ] {
        note(info, format!("{what} use: {}", allowed(flag)));
    }

    // An exported video is neither a redistribution nor a modification.
    note(
        info,
        format!(
            "Redistribution of the model: {} (a video made with it is not a redistribution)",
            allowed(meta.allow_redistribution)
        ),
    );
    let modification = match meta.modification.unwrap_or(ModificationType::Prohibited) {
        ModificationType::Prohibited => "not allowed",
        ModificationType::AllowModification => {
            "allowed, but the modified model may not be redistributed"
        }
        ModificationType::AllowModificationRedistribution => {
            "allowed, and the modified model may be redistributed"
        }
    };
    note(
        info,
        format!(
            "Modification of the model: {modification} (a video made with it is not a modification)"
        ),
    );
    notices
}

/// A text field that is there and not empty.
fn given(value: &Option<String>) -> Option<&str> {
    value.as_deref().filter(|value| !value.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::Document;
    use crate::synthetic::SyntheticVrm;
    use serde_json::{Value, json};

    fn notices(edit: impl FnOnce(&mut Value)) -> Vec<Notice> {
        let mut vrm = SyntheticVrm::avatar();
        edit(&mut vrm.meta);
        let document = Document::parse(&vrm.glb()).unwrap();
        usage_notices(&document.vrm.meta)
    }

    fn warnings(notices: &[Notice]) -> Vec<&str> {
        notices
            .iter()
            .filter(|notice| notice.level == NoticeLevel::Warning)
            .map(|notice| notice.message.as_str())
            .collect()
    }

    fn first_is_the_heading(notices: &[Notice]) {
        assert_eq!(notices[0].level, NoticeLevel::Info);
        assert!(notices[0].message.contains("Synthetic"), "{notices:?}");
    }

    #[test]
    fn terms_that_forbid_likely_video_use_warn() {
        // The synthetic terms allow a video.
        let permissive = notices(|_| {});
        first_is_the_heading(&permissive);
        assert!(warnings(&permissive).is_empty(), "{permissive:?}");

        // Absent, commercial use and credit take VRM 1.0's defaults.
        let defaults = notices(|meta| {
            let meta = meta.as_object_mut().unwrap();
            meta.remove("commercialUsage");
            meta.remove("creditNotation");
        });
        first_is_the_heading(&defaults);
        let found = warnings(&defaults);
        assert_eq!(found.len(), 2, "{found:?}");
        assert!(
            found[0].contains("Commercial use: not allowed"),
            "{found:?}"
        );
        assert!(
            found[1].contains("Credit: required; credit Celesta"),
            "{found:?}"
        );

        let author_only = notices(|meta| meta["avatarPermission"] = json!("onlyAuthor"));
        first_is_the_heading(&author_only);
        assert_eq!(warnings(&author_only), ["Avatar use: only the author"]);
        let licensed = notices(|meta| {
            meta.as_object_mut().unwrap().remove("avatarPermission");
        });
        assert_eq!(warnings(&licensed), ["Avatar use: only the author"]);
        let licensed = notices(|meta| {
            meta["avatarPermission"] = json!("onlySeparatelyLicensedPerson");
        });
        assert_eq!(
            warnings(&licensed),
            ["Avatar use: only people the author licenses separately"]
        );
    }

    #[test]
    fn every_term_is_a_line_in_order() {
        let notices = notices(|meta| {
            meta["version"] = json!("1.2");
            meta["otherLicenseUrl"] = json!("https://example.com/other");
            meta["copyrightInformation"] = json!("(c) Celesta");
            meta["contactInformation"] = json!("celesta@example.com");
            meta["references"] = json!(["Original", "Another"]);
            meta["thirdPartyLicenses"] = json!("Font: OFL");
            meta["commercialUsage"] = json!("personalProfit");
            meta["allowExcessivelySexualUsage"] = json!(true);
            meta["allowRedistribution"] = json!(true);
            meta["modification"] = json!("allowModification");
        });
        let messages: Vec<&str> = notices.iter().map(|n| n.message.as_str()).collect();
        assert_eq!(
            messages,
            [
                "VRM \"Synthetic\" 1.2 by Celesta; licence https://vrm.dev/licenses/1.0/; other licence https://example.com/other",
                "Copyright: (c) Celesta",
                "Contact: celesta@example.com",
                "References: Original; Another",
                "Third-party licences: Font: OFL",
                "Avatar use: anyone",
                "Commercial use: individuals only, not corporations",
                "Credit: not required",
                "Excessively violent use: not allowed",
                "Excessively sexual use: allowed",
                "Political or religious use: not allowed",
                "Antisocial or hateful use: not allowed",
                "Redistribution of the model: allowed (a video made with it is not a redistribution)",
                "Modification of the model: allowed, but the modified model may not be redistributed (a video made with it is not a modification)",
            ]
        );
        assert!(warnings(&notices).is_empty());
    }
}
