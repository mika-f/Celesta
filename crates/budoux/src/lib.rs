//! A Rust port of [BudouX](https://github.com/google/budoux), which splits
//! text into phrases so a line can break between them instead of anywhere
//! between two characters.
//!
//! The parser follows upstream's `budoux/parser.py`: it scores each position
//! between two characters with the model's unigram (`UW1`–`UW6`), bigram
//! (`BW1`–`BW3`), and trigram (`TW1`–`TW4`) weights around it and starts a
//! phrase where the score is positive. Characters are Unicode scalar values,
//! as in the Python version.
//!
//! `src/models/ja.json` is upstream's Japanese model, copied unchanged from
//! google/budoux 0.9.3 (commit 5c61e3c). BudouX and its models are
//! Copyright Google LLC, licensed under the Apache License, Version 2.0.

use std::collections::HashMap;
use std::sync::OnceLock;

/// The features a model weighs, in the order [`Parser`] stores them.
const FEATURES: [&str; 13] = [
    "UW1", "UW2", "UW3", "UW4", "UW5", "UW6", "BW1", "BW2", "BW3", "TW1", "TW2", "TW3", "TW4",
];
const UW1: usize = 0;
const UW2: usize = 1;
const UW3: usize = 2;
const UW4: usize = 3;
const UW5: usize = 4;
const UW6: usize = 5;
const BW1: usize = 6;
const BW2: usize = 7;
const BW3: usize = 8;
const TW1: usize = 9;
const TW2: usize = 10;
const TW3: usize = 11;
const TW4: usize = 12;

/// A BudouX model: each feature's weight for the characters around a
/// position.
pub struct Parser {
    /// `i32`, so neither the model's total nor a position's score can
    /// overflow the `i64` they are summed in.
    weights: [HashMap<String, i32>; 13],
    /// Twice upstream's base score (minus half the sum of every weight), so
    /// the score stays an integer.
    base_score: i64,
}

impl Parser {
    /// Reads a model in upstream's JSON format:
    /// `{ "UW1": { "あ": 123, … }, … }`. Weights must fit in an `i32`, as
    /// upstream's models' do.
    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        let mut model: HashMap<String, HashMap<String, i32>> = serde_json::from_str(json)?;
        // Every group counts towards the base score, including any this
        // parser does not read, as upstream sums the whole model.
        let total: i64 = model
            .values()
            .flat_map(HashMap::values)
            .map(|&weight| i64::from(weight))
            .sum();
        let weights = FEATURES.map(|feature| model.remove(feature).unwrap_or_default());
        Ok(Self {
            weights,
            base_score: -total,
        })
    }

    /// The parser with upstream's default Japanese model, read on first use.
    pub fn japanese() -> &'static Parser {
        static PARSER: OnceLock<Parser> = OnceLock::new();
        PARSER.get_or_init(|| {
            Parser::from_json(include_str!("models/ja.json")).expect("the bundled model is valid")
        })
    }

    /// Splits `sentence` into phrases. Joined, they are `sentence` again.
    pub fn parse<'a>(&self, sentence: &'a str) -> Vec<&'a str> {
        if sentence.is_empty() {
            return Vec::new();
        }
        let mut phrases = Vec::new();
        let mut start = 0;
        for boundary in self.parse_boundaries(sentence) {
            phrases.push(&sentence[start..boundary]);
            start = boundary;
        }
        phrases.push(&sentence[start..]);
        phrases
    }

    /// The byte offsets in `sentence` where a phrase starts, other than 0.
    pub fn parse_boundaries(&self, sentence: &str) -> Vec<usize> {
        // `offsets[i]` is where the `i`th character starts; the last entry
        // is the end of the sentence.
        let offsets: Vec<usize> = sentence
            .char_indices()
            .map(|(offset, _)| offset)
            .chain([sentence.len()])
            .collect();
        let length = offsets.len() - 1;
        let weight = |feature: usize, from: usize, to: usize| {
            self.weights[feature]
                .get(&sentence[offsets[from]..offsets[to]])
                .map_or(0, |&weight| i64::from(weight))
        };

        let mut boundaries = Vec::new();
        for i in 1..length {
            let mut score = 0;
            if i > 2 {
                score += weight(UW1, i - 3, i - 2);
            }
            if i > 1 {
                score += weight(UW2, i - 2, i - 1);
            }
            score += weight(UW3, i - 1, i);
            score += weight(UW4, i, i + 1);
            if i + 1 < length {
                score += weight(UW5, i + 1, i + 2);
            }
            if i + 2 < length {
                score += weight(UW6, i + 2, i + 3);
            }

            if i > 1 {
                score += weight(BW1, i - 2, i);
            }
            score += weight(BW2, i - 1, i + 1);
            if i + 1 < length {
                score += weight(BW3, i, i + 2);
            }

            if i > 2 {
                score += weight(TW1, i - 3, i);
            }
            if i > 1 {
                score += weight(TW2, i - 2, i + 1);
            }
            if i + 1 < length {
                score += weight(TW3, i - 1, i + 2);
            }
            if i + 2 < length {
                score += weight(TW4, i, i + 3);
            }

            if self.base_score + 2 * score > 0 {
                boundaries.push(offsets[i]);
            }
        }
        boundaries
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Ported from upstream's tests/test_parser.py.

    #[test]
    fn splits_where_a_strong_feature_supports_it() {
        // "Separate right before 'a'".
        let parser = Parser::from_json(r#"{ "UW4": { "a": 10000 } }"#).unwrap();
        assert_eq!(parser.parse("abcdeabcd"), ["abcde", "abcd"]);
    }

    #[test]
    fn splits_even_if_the_first_character_is_a_phrase_by_itself() {
        let parser = Parser::from_json(r#"{ "UW4": { "b": 10000 } }"#).unwrap();
        assert_eq!(parser.parse("abcdeabcd"), ["a", "bcdea", "bcd"]);
    }

    #[test]
    fn rejects_weights_outside_i32() {
        assert!(Parser::from_json(r#"{ "UW4": { "a": 9223372036854775807 } }"#).is_err());
    }

    #[test]
    fn returns_no_phrases_for_an_empty_sentence() {
        let parser = Parser::from_json("{}").unwrap();
        assert!(parser.parse("").is_empty());
        assert!(Parser::japanese().parse("").is_empty());
    }

    #[test]
    fn splits_with_the_default_japanese_model() {
        let sentence = "Google の使命は、世界中の情報を整理し、世界中の人がアクセスできて使えるようにすることです。";
        assert_eq!(
            Parser::japanese().parse(sentence),
            [
                "Google の",
                "使命は、",
                "世界中の",
                "情報を",
                "整理し、",
                "世界中の",
                "人が",
                "アクセスできて",
                "使えるように",
                "する",
                "ことです。",
            ]
        );
    }

    #[test]
    fn counts_characters_outside_the_bmp_as_one() {
        // Upstream's Python parser indexes by code point: "😀" is one
        // character, two before "z", so its `UW2` weight splits before "z".
        let parser = Parser::from_json(r#"{ "UW2": { "😀": 10000 } }"#).unwrap();
        assert_eq!(parser.parse("x😀yz"), ["x😀y", "z"]);
    }

    #[test]
    fn keeps_every_character() {
        let sentence =
            "フレームは時刻の関数なので、どのフレームからでも描き直せるのだ。\n改行も🇯🇵も残る";
        assert_eq!(Parser::japanese().parse(sentence).concat(), sentence);
    }
}
