//! The settings schema a media component declares for its options, and the
//! validation Celesta runs before any `open`.
//!
//! The shape follows `registerComponent()`'s fields (`ComponentPropertyField`
//! in `react-bridge`) so the editor's property controls can be reused.

use std::collections::BTreeMap;
use std::fmt;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// One setting's value: what a plugin reads in `options`.
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(untagged)]
pub enum SettingValue {
    String(String),
    Number(f64),
    Boolean(bool),
}

impl Serialize for SettingValue {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::String(value) => serializer.serialize_str(value),
            Self::Number(value) => serialize_number(*value, serializer),
            Self::Boolean(value) => serializer.serialize_bool(*value),
        }
    }
}

impl fmt::Display for SettingValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::String(value) => f.write_str(value),
            Self::Number(value) => write!(f, "{value}"),
            Self::Boolean(value) => write!(f, "{value}"),
        }
    }
}

/// A component's settings by key.
pub type Settings = BTreeMap<String, SettingValue>;

/// A whole number is written as a JSON integer (`18`, not `18.0`), so
/// plugins reading integers keep working.
fn serialize_number<S: Serializer>(value: f64, serializer: S) -> Result<S::Ok, S::Error> {
    // 2^53: every whole f64 below it is an exact i64.
    if value.is_finite() && value.fract() == 0.0 && value.abs() < 9_007_199_254_740_992.0 {
        serializer.serialize_i64(value as i64)
    } else {
        serializer.serialize_f64(value)
    }
}

fn serialize_optional_number<S: Serializer>(
    value: &Option<f64>,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    match value {
        Some(value) => serialize_number(*value, serializer),
        None => serializer.serialize_none(),
    }
}

fn is_false(value: &bool) -> bool {
    !*value
}

/// `visibleWhen` maps a key to one value or a list; the Rust side holds a list.
fn deserialize_visible_when<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<BTreeMap<String, Vec<SettingValue>>, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum OneOrMany {
        Many(Vec<SettingValue>),
        One(SettingValue),
    }
    let wire = BTreeMap::<String, OneOrMany>::deserialize(deserializer)?;
    Ok(wire
        .into_iter()
        .map(|(key, values)| {
            let values = match values {
                OneOrMany::Many(values) => values,
                OneOrMany::One(value) => vec![value],
            };
            (key, values)
        })
        .collect())
}

/// One declared setting.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingField {
    pub key: String,
    pub label: String,
    pub default: SettingValue,
    #[serde(flatten)]
    pub kind: SettingKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub advanced: bool,
    /// Every listed key must hold one of its values for the field to show and
    /// apply.
    #[serde(
        default,
        deserialize_with = "deserialize_visible_when",
        skip_serializing_if = "BTreeMap::is_empty"
    )]
    pub visible_when: BTreeMap<String, Vec<SettingValue>>,
}

/// A setting's type and the limits that go with it.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum SettingKind {
    Select {
        options: Vec<SelectOption>,
    },
    Number {
        #[serde(
            default,
            skip_serializing_if = "Option::is_none",
            serialize_with = "serialize_optional_number"
        )]
        min: Option<f64>,
        #[serde(
            default,
            skip_serializing_if = "Option::is_none",
            serialize_with = "serialize_optional_number"
        )]
        max: Option<f64>,
        #[serde(
            default,
            skip_serializing_if = "Option::is_none",
            serialize_with = "serialize_optional_number"
        )]
        step: Option<f64>,
        #[serde(default, skip_serializing_if = "is_false")]
        integer: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        unit: Option<String>,
    },
    Boolean,
    String,
}

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
pub struct SelectOption {
    pub value: String,
    pub label: String,
}

/// A setting that failed validation: the key and why.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error("{key}: {message}")]
pub struct InvalidSetting {
    pub key: String,
    pub message: String,
}

impl InvalidSetting {
    fn new(key: &str, message: impl Into<String>) -> Self {
        Self {
            key: key.to_owned(),
            message: message.into(),
        }
    }
}

fn find<'a>(schema: &'a [SettingField], key: &str) -> Result<&'a SettingField, InvalidSetting> {
    schema
        .iter()
        .find(|field| field.key == key)
        .ok_or_else(|| InvalidSetting::new(key, "unknown setting"))
}

/// Every field's default, overridden by `settings`.
fn effective(schema: &[SettingField], settings: &Settings) -> Settings {
    let mut values: Settings = schema
        .iter()
        .map(|field| (field.key.clone(), field.default.clone()))
        .collect();
    values.extend(settings.iter().map(|(k, v)| (k.clone(), v.clone())));
    values
}

/// Whether `field` shows (and applies) given `settings`: every `visibleWhen`
/// key holds one of its listed values. A key absent from `settings` matches
/// nothing.
pub fn is_visible(field: &SettingField, settings: &Settings) -> bool {
    field.visible_when.iter().all(|(key, values)| {
        settings
            .get(key)
            .is_some_and(|value| values.contains(value))
    })
}

fn check_value(field: &SettingField, value: &SettingValue) -> Result<(), InvalidSetting> {
    let fail = |message: String| Err(InvalidSetting::new(&field.key, message));
    match (&field.kind, value) {
        (SettingKind::Select { options }, SettingValue::String(text)) => {
            if options.iter().any(|option| &option.value == text) {
                Ok(())
            } else {
                let values: Vec<&str> = options.iter().map(|o| o.value.as_str()).collect();
                fail(format!("must be one of {}", values.join(", ")))
            }
        }
        (SettingKind::String, SettingValue::String(_)) => Ok(()),
        (SettingKind::Select { .. } | SettingKind::String, _) => {
            fail("must be a string".to_owned())
        }
        (SettingKind::Number { .. }, SettingValue::Number(number)) if !number.is_finite() => {
            fail("must be a number".to_owned())
        }
        (
            SettingKind::Number {
                min, max, integer, ..
            },
            SettingValue::Number(number),
        ) => {
            if *integer && number.fract() != 0.0 {
                fail("must be a whole number".to_owned())
            } else if min.is_some_and(|min| *number < min) {
                fail(format!("must be at least {}", min.unwrap_or_default()))
            } else if max.is_some_and(|max| *number > max) {
                fail(format!("must be at most {}", max.unwrap_or_default()))
            } else {
                Ok(())
            }
        }
        (SettingKind::Number { .. }, _) => fail("must be a number".to_owned()),
        (SettingKind::Boolean, SettingValue::Boolean(_)) => Ok(()),
        (SettingKind::Boolean, _) => fail("must be true or false".to_owned()),
    }
}

/// Checks `given` against `schema` and returns the full map: given values
/// checked, missing visible keys filled with their defaults, hidden fields
/// left out. Visibility is judged on the given values and defaults together.
pub fn validate(schema: &[SettingField], given: &Settings) -> Result<Settings, InvalidSetting> {
    let values = effective(schema, given);
    for (key, value) in given {
        let field = find(schema, key)?;
        if !is_visible(field, &values) {
            let (when_key, when_values) = field
                .visible_when
                .iter()
                .find(|(k, vs)| !values.get(*k).is_some_and(|value| vs.contains(value)))
                .expect("a hidden field has an unmet condition");
            let shown: Vec<String> = when_values.iter().map(ToString::to_string).collect();
            return Err(InvalidSetting::new(
                key,
                format!("applies only when {when_key} is {}", shown.join(" or ")),
            ));
        }
        check_value(field, value)?;
    }
    Ok(schema
        .iter()
        .filter(|field| is_visible(field, &values))
        .map(|field| (field.key.clone(), values[&field.key].clone()))
        .collect())
}

/// Reads command-line text as the field's type: a number or a boolean
/// parses, anything else is a string.
pub fn parse_setting(
    schema: &[SettingField],
    key: &str,
    text: &str,
) -> Result<SettingValue, InvalidSetting> {
    match find(schema, key)?.kind {
        SettingKind::Number { .. } => text
            .trim()
            .parse::<f64>()
            .map(SettingValue::Number)
            .map_err(|_| InvalidSetting::new(key, "must be a number")),
        SettingKind::Boolean => match text.trim() {
            "true" => Ok(SettingValue::Boolean(true)),
            "false" => Ok(SettingValue::Boolean(false)),
            _ => Err(InvalidSetting::new(key, "must be true or false")),
        },
        SettingKind::Select { .. } | SettingKind::String => {
            Ok(SettingValue::String(text.to_owned()))
        }
    }
}

/// The visible fields whose value in `settings` is not the default, in schema
/// order.
pub fn differences<'a>(
    schema: &'a [SettingField],
    settings: &'a Settings,
) -> Vec<(&'a SettingField, &'a SettingValue)> {
    let values = effective(schema, settings);
    schema
        .iter()
        .filter(|field| is_visible(field, &values))
        .filter_map(|field| {
            let value = settings.get(&field.key)?;
            (*value != field.default).then_some((field, value))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};

    fn spec_json() -> Vec<Value> {
        vec![
            json!({ "key": "rateControl", "type": "select", "label": "Rate control",
                "group": "Rate control",
                "options": [ { "value": "crf", "label": "Constant quality (CRF)" },
                             { "value": "vbr", "label": "Average bitrate" } ],
                "default": "crf" }),
            json!({ "key": "crf", "type": "number", "integer": true, "min": 0, "max": 51,
                "step": 1, "default": 18, "label": "CRF", "group": "Rate control",
                "visibleWhen": { "rateControl": "crf" },
                "description": "Lower is higher quality and larger." }),
            json!({ "key": "bitrate", "type": "number", "integer": true, "min": 100000,
                "unit": "bit/s", "default": 8000000, "label": "Bitrate",
                "group": "Rate control", "visibleWhen": { "rateControl": "vbr" } }),
            json!({ "key": "tune", "type": "string", "default": "", "label": "Tune",
                "advanced": true }),
            json!({ "key": "faststart", "type": "boolean", "default": true,
                "label": "Fast start" }),
        ]
    }

    fn schema() -> Vec<SettingField> {
        spec_json()
            .into_iter()
            .map(|json| serde_json::from_value(json).expect("a spec field"))
            .collect()
    }

    fn given(entries: Value) -> Settings {
        serde_json::from_value(entries).expect("settings")
    }

    fn invalid(schema: &[SettingField], entries: Value) -> (String, String) {
        let error = validate(schema, &given(entries)).expect_err("invalid");
        (error.key, error.message)
    }

    fn num(value: f64) -> SettingValue {
        SettingValue::Number(value)
    }

    #[test]
    fn the_spec_fields_round_trip_json() {
        for mut expected in spec_json() {
            let field: SettingField = serde_json::from_value(expected.clone()).unwrap();
            if let Some(when) = expected.get_mut("visibleWhen") {
                for value in when.as_object_mut().unwrap().values_mut() {
                    *value = json!([value.clone()]);
                }
            }
            assert_eq!(serde_json::to_value(&field).unwrap(), expected);
        }
        let crf = serde_json::to_string(&schema()[1]).unwrap();
        assert!(crf.contains("\"default\":18,") && crf.contains("\"max\":51"));
    }

    #[test]
    fn defaults_fill_missing_keys() {
        let settings = validate(&schema(), &Settings::new()).unwrap();
        assert_eq!(settings["rateControl"], SettingValue::String("crf".into()));
        assert_eq!(settings["crf"], num(18.0));
        assert_eq!(settings["tune"], SettingValue::String(String::new()));
        assert_eq!(settings["faststart"], SettingValue::Boolean(true));
        assert!(!settings.contains_key("bitrate"));
    }

    #[test]
    fn a_hidden_field_may_not_be_given() {
        assert_eq!(
            invalid(&schema(), json!({ "bitrate": 4000000 })),
            (
                "bitrate".into(),
                "applies only when rateControl is vbr".into()
            )
        );
    }

    #[test]
    fn switching_rate_control_shows_bitrate() {
        let settings = validate(&schema(), &given(json!({ "rateControl": "vbr" }))).unwrap();
        assert_eq!(settings["bitrate"], num(8_000_000.0));
        assert!(!settings.contains_key("crf"));
    }

    #[test]
    fn numbers_are_ranged_and_whole() {
        let schema = schema();
        assert_eq!(
            invalid(&schema, json!({ "crf": 52 })).1,
            "must be at most 51"
        );
        assert_eq!(
            invalid(&schema, json!({ "crf": 17.5 })).1,
            "must be a whole number"
        );
        assert_eq!(
            invalid(&schema, json!({ "crf": "18" })).1,
            "must be a number"
        );
        assert_eq!(
            invalid(&schema, json!({ "rateControl": "vbr", "bitrate": 5 })).1,
            "must be at least 100000"
        );
    }

    #[test]
    fn selects_take_their_options() {
        assert_eq!(
            invalid(&schema(), json!({ "rateControl": "abr" })),
            ("rateControl".into(), "must be one of crf, vbr".into())
        );
    }

    #[test]
    fn unknown_keys_fail() {
        assert_eq!(
            invalid(&schema(), json!({ "quality": 1 })),
            ("quality".into(), "unknown setting".into())
        );
    }

    #[test]
    fn strings_and_booleans_are_typed() {
        let schema = schema();
        assert_eq!(invalid(&schema, json!({ "tune": 1 })).1, "must be a string");
        assert_eq!(
            invalid(&schema, json!({ "faststart": "yes" })).1,
            "must be true or false"
        );
    }

    #[test]
    fn parse_setting_follows_the_type() {
        let schema = schema();
        assert_eq!(parse_setting(&schema, "crf", "20").unwrap(), num(20.0));
        assert_eq!(
            parse_setting(&schema, "faststart", "false").unwrap(),
            SettingValue::Boolean(false)
        );
        assert_eq!(
            parse_setting(&schema, "tune", "film").unwrap(),
            SettingValue::String("film".into())
        );
        assert_eq!(
            parse_setting(&schema, "crf", "x").unwrap_err().message,
            "must be a number"
        );
    }

    #[test]
    fn differences_list_what_is_not_default() {
        let schema = schema();
        let settings = given(json!({ "crf": 20, "faststart": true }));
        assert_eq!(
            differences(&schema, &settings),
            vec![(&schema[1], &num(20.0))]
        );
    }
}
