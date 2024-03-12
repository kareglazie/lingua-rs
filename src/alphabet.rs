/*
 * Copyright © 2020-present Peter M. Stahl pemistahl@gmail.com
 *
 * Licensed under the Apache License, Version 2.0 (the "License");
 * you may not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *
 * http://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing, software
 * distributed under the License is distributed on an "AS IS" BASIS,
 * WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either expressed or implied.
 * See the License for the specific language governing permissions and
 * limitations under the License.
 */

use std::collections::HashMap;

use ahash::AHashSet;
use once_cell::sync::Lazy;
use strum::IntoEnumIterator;
use strum_macros::EnumIter;
use crate::Language;

#[derive(EnumIter, Eq, PartialEq, Hash)]
pub(crate) enum Alphabet {
    Arabic,
    Armenian,
    Bengali,
    Cyrillic,
    Devanagari,
    Georgian,
    Greek,
    Gujarati,
    Gurmukhi,
    Han,
    Hangul,
    Hebrew,
    Hiragana,
    Katakana,
    Latin,
    Tamil,
    Telugu,
    Thai,
    Ethiopic,
    Myanmar,
    Malayalam,
    Sinhala,
}

impl Alphabet {
    pub fn matches(&self, text: &str) -> bool {
        self.char_set().is_match(text)
    }

    pub fn matches_char(&self, ch: char) -> bool {
        self.char_set().is_char_match(ch)
    }

    pub fn all_supporting_single_language() -> HashMap<Alphabet, Language> {
        let mut alphabets = HashMap::new();
        for alphabet in Alphabet::iter() {
            let supported_languages = alphabet.supported_languages();
            if supported_languages.len() == 1 {
                alphabets.insert(alphabet, supported_languages[0]);
            }
        }
        alphabets
    }

    fn supported_languages(&self) -> Vec<Language> {
        let mut languages = vec![];
        for language in Language::iter() {
            if language.alphabets().contains(self) {
                languages.push(language);
            }
        }
        languages
    }

    fn char_set(&self) -> &Lazy<CharSet> {
        match self {
            Alphabet::Arabic => &ARABIC,
            Alphabet::Armenian => &ARMENIAN,
            Alphabet::Bengali => &BENGALI,
            Alphabet::Cyrillic => &CYRILLIC,
            Alphabet::Devanagari => &DEVANAGARI,
            Alphabet::Georgian => &GEORGIAN,
            Alphabet::Greek => &GREEK,
            Alphabet::Gujarati => &GUJARATI,
            Alphabet::Gurmukhi => &GURMUKHI,
            Alphabet::Han => &HAN,
            Alphabet::Hangul => &HANGUL,
            Alphabet::Hebrew => &HEBREW,
            Alphabet::Hiragana => &HIRAGANA,
            Alphabet::Katakana => &KATAKANA,
            Alphabet::Latin => &LATIN,
            Alphabet::Tamil => &TAMIL,
            Alphabet::Telugu => &TELUGU,
            Alphabet::Thai => &THAI,
            Alphabet::Ethiopic => &ETHIOPIC,
            Alphabet::Myanmar => &MYANMAR,
            Alphabet::Malayalam => &MALAYALAM,
            Alphabet::Sinhala => &SINHALA,
        }
    }
}

pub(crate) struct CharSet {
    characters: AHashSet<char>,
}

impl CharSet {
    pub fn from_char_classes(char_classes: &[&str]) -> Self {
        let mut characters = AHashSet::new();

        for char_class in char_classes {
            let table = crate::script::BY_NAME
                .iter()
                .find(|(name, _)| *name == *char_class)
                .unwrap()
                .1;

            for &(start, end) in table {
                for codepoint in start..=end {
                    characters.insert(codepoint);
                }
            }
        }

        CharSet { characters }
    }

    pub fn from_char_class(char_class: &str) -> Self {
        Self::from_char_classes(&[char_class])
    }

    pub fn is_match(&self, text: &str) -> bool {
        text.chars().all(|ch| self.is_char_match(ch))
    }

    pub fn is_char_match(&self, ch: char) -> bool {
        self.characters.contains(&ch)
    }
}

static ARABIC: Lazy<CharSet> = Lazy::new(|| CharSet::from_char_class("Arabic"));
static ARMENIAN: Lazy<CharSet> = Lazy::new(|| CharSet::from_char_class("Armenian"));
static BENGALI: Lazy<CharSet> = Lazy::new(|| CharSet::from_char_class("Bengali"));
static CYRILLIC: Lazy<CharSet> = Lazy::new(|| CharSet::from_char_class("Cyrillic"));
static DEVANAGARI: Lazy<CharSet> = Lazy::new(|| CharSet::from_char_class("Devanagari"));
static GEORGIAN: Lazy<CharSet> = Lazy::new(|| CharSet::from_char_class("Georgian"));
static GREEK: Lazy<CharSet> = Lazy::new(|| CharSet::from_char_class("Greek"));
static GUJARATI: Lazy<CharSet> = Lazy::new(|| CharSet::from_char_class("Gujarati"));
static GURMUKHI: Lazy<CharSet> = Lazy::new(|| CharSet::from_char_class("Gurmukhi"));
static HAN: Lazy<CharSet> = Lazy::new(|| CharSet::from_char_class("Han"));
static HANGUL: Lazy<CharSet> = Lazy::new(|| CharSet::from_char_class("Hangul"));
static HEBREW: Lazy<CharSet> = Lazy::new(|| CharSet::from_char_class("Hebrew"));
static HIRAGANA: Lazy<CharSet> = Lazy::new(|| CharSet::from_char_class("Hiragana"));
static KATAKANA: Lazy<CharSet> = Lazy::new(|| CharSet::from_char_class("Katakana"));
static LATIN: Lazy<CharSet> = Lazy::new(|| CharSet::from_char_class("Latin"));
static TAMIL: Lazy<CharSet> = Lazy::new(|| CharSet::from_char_class("Tamil"));
static TELUGU: Lazy<CharSet> = Lazy::new(|| CharSet::from_char_class("Telugu"));
static THAI: Lazy<CharSet> = Lazy::new(|| CharSet::from_char_class("Thai"));
static MYANMAR: Lazy<CharSet> = Lazy::new(|| CharSet::from_char_class("Myanmar"));
static ETHIOPIC: Lazy<CharSet> = Lazy::new(|| CharSet::from_char_class("Ethiopic"));
static MALAYALAM: Lazy<CharSet> = Lazy::new(|| CharSet::from_char_class("Malayalam"));
static SINHALA: Lazy<CharSet> = Lazy::new(|| CharSet::from_char_class("Sinhala"));
