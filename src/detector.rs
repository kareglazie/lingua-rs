/*
 * Copyright © 2020 Peter M. Stahl pemistahl@gmail.com
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

use crate::alphabet::Alphabet;
use crate::constant::{
    CHARS_TO_LANGUAGES_MAPPING, JAPANESE_CHARACTER_SET, MULTIPLE_WHITESPACE, NO_LETTER, NUMBERS,
    PUNCTUATION,
};
use crate::language::Language;
use crate::language::Language::*;
use crate::model::TestDataLanguageModel;
use crate::models::bigram_models::bigram_models;
use crate::models::fivegram_models::fivegram_models;
use crate::models::quadrigram_models::quadrigram_models;
use crate::models::trigram_models::trigram_models;
use crate::models::unigram_models::unigram_models;
use crate::models::LazyLanguageToNgramsMapping;
use crate::ngram::Ngram;
use cfg_if::cfg_if;
use itertools::Itertools;
use std::collections::{HashMap, HashSet};
use std::hash::Hash;
use strum::IntoEnumIterator;

cfg_if! {
    if #[cfg(test)] {
        use crate::model::MockTrainingDataLanguageModel as TrainingDataLanguageModel;
    } else {
        #[allow(unused_imports)]
        use crate::model::TrainingDataLanguageModel;
    }
}

/// This struct detects the language of given input text.
pub struct LanguageDetector {
    languages: HashSet<Language>,
    minimum_relative_distance: f64,
    languages_with_unique_characters: HashSet<Language>,
    one_language_alphabets: HashMap<Alphabet, Language>,
    unigram_language_models: LazyLanguageToNgramsMapping,
    bigram_language_models: LazyLanguageToNgramsMapping,
    trigram_language_models: LazyLanguageToNgramsMapping,
    quadrigram_language_models: LazyLanguageToNgramsMapping,
    fivegram_language_models: LazyLanguageToNgramsMapping,
}

impl LanguageDetector {
    pub(crate) fn from(languages: HashSet<Language>, minimum_relative_distance: f64) -> Self {
        let languages_with_unique_characters = languages
            .iter()
            .filter(|it| it.unique_characters().is_some())
            .cloned()
            .collect();
        let one_language_alphabets = Alphabet::all_supporting_single_language()
            .into_iter()
            .filter(|(_, language)| languages.contains(language))
            .collect();
        Self {
            languages,
            minimum_relative_distance,
            languages_with_unique_characters,
            one_language_alphabets,
            unigram_language_models: unigram_models(),
            bigram_language_models: bigram_models(),
            trigram_language_models: trigram_models(),
            quadrigram_language_models: quadrigram_models(),
            fivegram_language_models: fivegram_models(),
        }
    }

    /// Detects the language of given input text.
    /// If the language cannot be reliably detected, `None` is returned.
    pub fn detect_language_of<T: Into<String>>(&self, text: T) -> Option<Language> {
        let confidence_values = self.compute_language_confidence_values(text);

        if confidence_values.is_empty() {
            return None;
        }

        let (most_likely_language, most_likely_language_probability) =
            &confidence_values.first().unwrap();

        if confidence_values.len() == 1 {
            return Some(most_likely_language.clone());
        }

        let (_, second_most_likely_language_probability) = &confidence_values.get(1).unwrap();

        if (most_likely_language_probability - second_most_likely_language_probability).abs()
            < f64::EPSILON
        {
            return None;
        }

        if (most_likely_language_probability - second_most_likely_language_probability)
            < self.minimum_relative_distance
        {
            return None;
        }

        Some(most_likely_language.clone())
    }

    /// Computes confidence values for each language considered possible for the given input text.
    ///
    /// A vector of all possible languages is returned, sorted by their confidence value in
    /// descending order. The values that this method computes are part of a **relative**
    /// confidence metric, not of an absolute one. Each value is a number between 0.0 and 1.0.
    /// The most likely language is always returned with value 1.0. All other languages get values
    /// assigned which are lower than 1.0, denoting how less likely those languages are in
    /// comparison to the most likely language.
    ///
    /// The vector returned by this method does not necessarily contain all languages which the
    /// calling instance of `LanguageDetector` was built from. If the rule-based engine decides
    /// that a specific language is truly impossible, then it will not be part of the returned
    /// vector. Likewise, if no ngram probabilities can be found within the detector's languages
    /// for the given input text, the returned vector will be empty. The confidence value for
    /// each language not being part of the returned vector is assumed to be 0.0.
    pub fn compute_language_confidence_values<T: Into<String>>(
        &self,
        text: T,
    ) -> Vec<(Language, f64)> {
        let mut values = vec![];
        let cleaned_up_text = self.clean_up_input_text(text.into());

        if cleaned_up_text.is_empty() || NO_LETTER.is_match(&cleaned_up_text) {
            return values;
        }

        let words = self.split_text_into_words(&cleaned_up_text);
        let language_detected_by_rules = self.detect_language_with_rules(&words);

        if let Some(language) = language_detected_by_rules {
            values.push((language, 1.0));
            return values;
        }

        let mut filtered_languages = self.filter_languages_by_rules(words);

        if filtered_languages.len() == 1 {
            let filtered_language = filtered_languages.into_iter().next().unwrap();
            values.push((filtered_language, 1.0));
            return values;
        }

        let mut all_probabilities = Vec::<HashMap<Language, f64>>::new();
        let mut unigram_counts = HashMap::<Language, u32>::new();

        for i in 1..6 {
            if cleaned_up_text.chars().count() < i {
                continue;
            }
            let test_data_model = TestDataLanguageModel::from(&cleaned_up_text, i);
            let probabilities =
                self.compute_language_probabilities(&test_data_model, &filtered_languages);
            let languages = probabilities.keys().collect_vec();

            if !languages.is_empty() {
                filtered_languages = filtered_languages
                    .into_iter()
                    .filter(|it| languages.contains(&it))
                    .collect();
            }

            if i == 1 {
                self.count_unigrams(&mut unigram_counts, &test_data_model, &filtered_languages);
            }

            all_probabilities.push(probabilities);
        }

        let summed_up_probabilities =
            self.sum_up_probabilities(all_probabilities, unigram_counts, filtered_languages);

        if summed_up_probabilities.is_empty() {
            return values;
        }

        let highest_probability = summed_up_probabilities
            .iter()
            .map(|(_, &probability)| probability)
            .sorted_by(|&first, &second| second.partial_cmp(&first).unwrap())
            .next()
            .unwrap();

        summed_up_probabilities
            .into_iter()
            .map(|(language, probability)| (language, highest_probability / probability))
            .sorted_by(
                |(first_language, first_probability), (second_language, second_probability)| {
                    let sorted_by_probability =
                        second_probability.partial_cmp(first_probability).unwrap();
                    let sorted_by_language = first_language.partial_cmp(second_language).unwrap();

                    sorted_by_probability.then(sorted_by_language)
                },
            )
            .collect_vec()
    }

    fn clean_up_input_text(&self, text: String) -> String {
        let trimmed = text.trim().to_lowercase();
        let without_punctuation = PUNCTUATION.replace_all(&trimmed, "");
        let without_numbers = NUMBERS.replace_all(&without_punctuation, "");
        let normalized_whitespace = MULTIPLE_WHITESPACE.replace_all(&without_numbers, " ");
        normalized_whitespace.to_string()
    }

    fn split_text_into_words<'a>(&self, text: &'a str) -> Vec<&'a str> {
        if text.contains(' ') {
            text.split(' ').collect_vec()
        } else {
            vec![text]
        }
    }

    fn detect_language_with_rules(&self, words: &[&str]) -> Option<Language> {
        let mut total_language_counts = HashMap::<Option<&Language>, u32>::new();
        let half_word_count = (words.len() as f64) * 0.5;

        for word in words {
            let mut word_language_counts = HashMap::<&Language, u32>::new();

            for character in word.chars() {
                let mut is_match = false;
                let mut buffer = [0; 4];
                let char_str = character.encode_utf8(&mut buffer);

                for (alphabet, language) in self.one_language_alphabets.iter() {
                    if alphabet.matches(char_str) {
                        self.increment_counter(&mut word_language_counts, language);
                        is_match = true;
                    }
                }

                if !is_match {
                    if Alphabet::Han.matches(char_str) {
                        self.increment_counter(&mut word_language_counts, &Chinese);
                    } else if JAPANESE_CHARACTER_SET.is_match(char_str) {
                        self.increment_counter(&mut word_language_counts, &Japanese);
                    } else if Alphabet::Latin.matches(char_str)
                        || Alphabet::Cyrillic.matches(char_str)
                        || Alphabet::Devanagari.matches(char_str)
                    {
                        self.languages_with_unique_characters
                            .iter()
                            .filter(|it| it.unique_characters().unwrap().contains(character))
                            .for_each(|it| self.increment_counter(&mut word_language_counts, it));
                    }
                }
            }

            if word_language_counts.is_empty() {
                self.increment_counter(&mut total_language_counts, None);
            } else if word_language_counts.len() == 1 {
                let counted_languages = word_language_counts.keys().collect_vec();
                let language = counted_languages.first().unwrap();
                if self.languages.contains(language) {
                    self.increment_counter(&mut total_language_counts, Some(language));
                } else {
                    self.increment_counter(&mut total_language_counts, None);
                }
            } else if word_language_counts.contains_key(&Chinese)
                && word_language_counts.contains_key(&Japanese)
            {
                self.increment_counter(&mut total_language_counts, Some(&Japanese));
            } else {
                let sorted_word_language_counts = word_language_counts
                    .into_iter()
                    .sorted_by(|(_, first_count), (_, second_count)| second_count.cmp(first_count))
                    .collect_vec();
                let (most_frequent_language, first_count) = sorted_word_language_counts[0];
                let (_, second_count) = sorted_word_language_counts[1];

                if first_count > second_count && self.languages.contains(most_frequent_language) {
                    self.increment_counter(
                        &mut total_language_counts,
                        Some(most_frequent_language),
                    );
                } else {
                    self.increment_counter(&mut total_language_counts, None);
                }
            }
        }

        let unknown_language_count = *total_language_counts.get(&None).or(Some(&0)).unwrap() as f64;

        if unknown_language_count < half_word_count {
            total_language_counts.remove(&None);
        }

        if total_language_counts.is_empty() {
            return None;
        }

        if total_language_counts.len() == 1 {
            return total_language_counts.iter().next().unwrap().0.cloned();
        }

        let sorted_total_language_counts = total_language_counts
            .into_iter()
            .sorted_by(|(_, first_count), (_, second_count)| second_count.cmp(first_count))
            .collect_vec();
        let (most_frequent_language, first_count) = sorted_total_language_counts[0];
        let (_, second_count) = sorted_total_language_counts[1];

        if first_count == second_count {
            return None;
        }

        most_frequent_language.cloned()
    }

    fn filter_languages_by_rules(&self, words: Vec<&str>) -> HashSet<Language> {
        let mut detected_alphabets = HashMap::<Alphabet, u32>::new();
        let half_word_count = (words.len() as f64) * 0.5;

        for word in words.iter() {
            for alphabet in Alphabet::iter() {
                if alphabet.matches(word) {
                    self.increment_counter(&mut detected_alphabets, alphabet);
                    break;
                }
            }
        }

        if detected_alphabets.is_empty() {
            return self.languages.clone();
        }

        let most_frequent_alphabet = detected_alphabets
            .into_iter()
            .sorted_by(|(_, first_count), (_, second_count)| second_count.cmp(first_count))
            .next()
            .unwrap()
            .0;

        let filtered_languages = self
            .languages
            .iter()
            .cloned()
            .filter(|it| it.alphabets().contains(&most_frequent_alphabet))
            .collect::<HashSet<_>>();

        let mut language_counts = HashMap::<&Language, u32>::new();

        for word in words.iter() {
            for (characters, languages) in CHARS_TO_LANGUAGES_MAPPING.iter() {
                let mut word_contains_char = false;
                for character in characters.chars() {
                    if word.contains(character) {
                        for language in languages.iter() {
                            self.increment_counter(&mut language_counts, language);
                        }
                        word_contains_char = true;
                        break;
                    }
                }
                if word_contains_char {
                    break;
                }
            }
        }

        let languages_subset = language_counts
            .into_iter()
            .filter(|(_, count)| (*count as f64) >= half_word_count)
            .map(|(language, _)| language)
            .collect::<HashSet<_>>();

        if !languages_subset.is_empty() {
            filtered_languages
                .into_iter()
                .filter(|it| languages_subset.contains(&it))
                .collect::<HashSet<_>>()
        } else {
            filtered_languages
        }
    }

    fn compute_language_probabilities(
        &self,
        model: &TestDataLanguageModel,
        filtered_languages: &HashSet<Language>,
    ) -> HashMap<Language, f64> {
        let mut probabilities = hashmap!();
        for language in filtered_languages.iter() {
            let sum = self.compute_sum_of_ngram_probabilities(language, &model.ngrams);
            if sum < 0.0 {
                probabilities.insert(language.clone(), sum);
            }
        }
        probabilities
    }

    fn compute_sum_of_ngram_probabilities(
        &self,
        language: &Language,
        ngrams: &HashSet<Ngram>,
    ) -> f64 {
        let mut probabilities = vec![];
        for ngram in ngrams.iter() {
            for elem in ngram.range_of_lower_order_ngrams() {
                let probability = self.look_up_ngram_probability(language, &elem);

                if probability > 0.0 {
                    probabilities.push(probability);
                    break;
                }
            }
        }
        probabilities.into_iter().map(|it| it.ln()).sum()
    }

    fn look_up_ngram_probability(&self, language: &Language, ngram: &Ngram) -> f64 {
        let language_models = match ngram.value.chars().count() {
            5 => self.fivegram_language_models,
            4 => self.quadrigram_language_models,
            3 => self.trigram_language_models,
            2 => self.bigram_language_models,
            1 => self.unigram_language_models,
            0 => panic!("zerogram detected"),
            _ => panic!(
                "unsupported ngram length detected: {}",
                ngram.value.chars().count()
            ),
        };

        language_models
            .get(language)
            .unwrap()
            .get_relative_frequency(ngram)
    }

    fn count_unigrams(
        &self,
        unigram_counts: &mut HashMap<Language, u32>,
        unigram_model: &TestDataLanguageModel,
        filtered_languages: &HashSet<Language>,
    ) {
        for language in filtered_languages.iter() {
            for unigram in unigram_model.ngrams.iter() {
                if self.look_up_ngram_probability(language, unigram) > 0.0 {
                    self.increment_counter(unigram_counts, language.clone());
                }
            }
        }
    }

    fn sum_up_probabilities(
        &self,
        probabilities: Vec<HashMap<Language, f64>>,
        unigram_counts: HashMap<Language, u32>,
        filtered_languages: HashSet<Language>,
    ) -> HashMap<Language, f64> {
        let mut summed_up_probabilities = hashmap!();
        for language in filtered_languages.iter() {
            let mut sum = probabilities
                .iter()
                .map(|it| match it.get(language) {
                    Some(probability) => *probability,
                    None => 0.0,
                })
                .sum();

            if unigram_counts.contains_key(language) {
                sum /= *unigram_counts.get(language).unwrap() as f64;
            }

            if sum != 0.0 {
                summed_up_probabilities.insert(language.clone(), sum);
            }
        }

        summed_up_probabilities
    }

    fn increment_counter<T: Eq + Hash>(&self, counts: &mut HashMap<T, u32>, key: T) {
        let counter = counts.entry(key).or_insert(0);
        *counter += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{LanguageToNgramsMappingCell, LazyTrainingDataLanguageModel};
    use float_cmp::approx_eq;
    use once_cell::sync::OnceCell;
    use rstest::*;

    // ##############################
    // MOCKS
    // ##############################

    fn create_training_model_mock(data: HashMap<&'static str, f64>) -> TrainingDataLanguageModel {
        let mut mock = TrainingDataLanguageModel::new();
        for (ngram, probability) in data {
            mock.expect_get_relative_frequency()
                .withf(move |n| n == &Ngram::new(ngram))
                .return_const(probability);
        }
        mock
    }

    // ##############################
    // LANGUAGE MODELS FOR ENGLISH
    // ##############################

    #[fixture]
    fn unigram_language_model_for_english() -> LazyTrainingDataLanguageModel {
        static ENGLISH_UNIGRAM_MODEL_FIXTURE: OnceCell<TrainingDataLanguageModel> = OnceCell::new();
        ENGLISH_UNIGRAM_MODEL_FIXTURE.get_or_init(|| {
            create_training_model_mock(hashmap!(
                "a" => 0.01,
                "l" => 0.02,
                "t" => 0.03,
                "e" => 0.04,
                "r" => 0.05,
                // unknown unigrams
                "w" => 0.0
            ))
        })
    }

    #[fixture]
    fn bigram_language_model_for_english() -> LazyTrainingDataLanguageModel {
        static ENGLISH_BIGRAM_MODEL_FIXTURE: OnceCell<TrainingDataLanguageModel> = OnceCell::new();
        ENGLISH_BIGRAM_MODEL_FIXTURE.get_or_init(|| {
            create_training_model_mock(hashmap!(
                "al" => 0.11,
                "lt" => 0.12,
                "te" => 0.13,
                "er" => 0.14,
                // unknown bigrams
                "aq" => 0.0,
                "wx" => 0.0
            ))
        })
    }

    #[fixture]
    fn trigram_language_model_for_english() -> LazyTrainingDataLanguageModel {
        static ENGLISH_TRIGRAM_MODEL_FIXTURE: OnceCell<TrainingDataLanguageModel> = OnceCell::new();
        ENGLISH_TRIGRAM_MODEL_FIXTURE.get_or_init(|| {
            create_training_model_mock(hashmap!(
                "alt" => 0.19,
                "lte" => 0.2,
                "ter" => 0.21,
                // unknown trigrams
                "aqu" => 0.0,
                "tez" => 0.0,
                "wxy" => 0.0
            ))
        })
    }

    #[fixture]
    fn quadrigram_language_model_for_english() -> LazyTrainingDataLanguageModel {
        static ENGLISH_QUADRIGRAM_MODEL_FIXTURE: OnceCell<TrainingDataLanguageModel> =
            OnceCell::new();
        ENGLISH_QUADRIGRAM_MODEL_FIXTURE.get_or_init(|| {
            create_training_model_mock(hashmap!(
                "alte" => 0.25,
                "lter" => 0.26,
                // unknown quadrigrams
                "aqua" => 0.0,
                "wxyz" => 0.0
            ))
        })
    }

    #[fixture]
    fn fivegram_language_model_for_english() -> LazyTrainingDataLanguageModel {
        static ENGLISH_FIVEGRAM_MODEL_FIXTURE: OnceCell<TrainingDataLanguageModel> =
            OnceCell::new();
        ENGLISH_FIVEGRAM_MODEL_FIXTURE.get_or_init(|| {
            create_training_model_mock(hashmap!(
                "alter" => 0.29,
                // unknown fivegrams
                "aquas" => 0.0
            ))
        })
    }

    // ##############################
    // LANGUAGE MODELS FOR GERMAN
    // ##############################

    #[fixture]
    fn unigram_language_model_for_german() -> LazyTrainingDataLanguageModel {
        static GERMAN_UNIGRAM_MODEL_FIXTURE: OnceCell<TrainingDataLanguageModel> = OnceCell::new();
        GERMAN_UNIGRAM_MODEL_FIXTURE.get_or_init(|| {
            create_training_model_mock(hashmap!(
                "a" => 0.06,
                "l" => 0.07,
                "t" => 0.08,
                "e" => 0.09,
                "r" => 0.1,
                // unknown unigrams
                "w" => 0.0
            ))
        })
    }

    #[fixture]
    fn bigram_language_model_for_german() -> LazyTrainingDataLanguageModel {
        static GERMAN_BIGRAM_MODEL_FIXTURE: OnceCell<TrainingDataLanguageModel> = OnceCell::new();
        GERMAN_BIGRAM_MODEL_FIXTURE.get_or_init(|| {
            create_training_model_mock(hashmap!(
                "al" => 0.15,
                "lt" => 0.16,
                "te" => 0.17,
                "er" => 0.18,
                // unknown bigrams
                "wx" => 0.0
            ))
        })
    }

    #[fixture]
    fn trigram_language_model_for_german() -> LazyTrainingDataLanguageModel {
        static GERMAN_TRIGRAM_MODEL_FIXTURE: OnceCell<TrainingDataLanguageModel> = OnceCell::new();
        GERMAN_TRIGRAM_MODEL_FIXTURE.get_or_init(|| {
            create_training_model_mock(hashmap!(
                "alt" => 0.22,
                "lte" => 0.23,
                "ter" => 0.24,
                // unknown trigrams
                "wxy" => 0.0
            ))
        })
    }

    #[fixture]
    fn quadrigram_language_model_for_german() -> LazyTrainingDataLanguageModel {
        static GERMAN_QUADRIGRAM_MODEL_FIXTURE: OnceCell<TrainingDataLanguageModel> =
            OnceCell::new();
        GERMAN_QUADRIGRAM_MODEL_FIXTURE.get_or_init(|| {
            create_training_model_mock(hashmap!(
                "alte" => 0.27,
                "lter" => 0.28,
                // unknown quadrigrams
                "wxyz" => 0.0
            ))
        })
    }

    #[fixture]
    fn fivegram_language_model_for_german() -> LazyTrainingDataLanguageModel {
        static GERMAN_FIVEGRAM_MODEL_FIXTURE: OnceCell<TrainingDataLanguageModel> = OnceCell::new();
        GERMAN_FIVEGRAM_MODEL_FIXTURE
            .get_or_init(|| create_training_model_mock(hashmap!("alter" => 0.3)))
    }

    // ##############################
    // NGRAM MODELS
    // ##############################

    #[fixture]
    fn unigram_language_models(
        unigram_language_model_for_english: LazyTrainingDataLanguageModel,
        unigram_language_model_for_german: LazyTrainingDataLanguageModel,
    ) -> LazyLanguageToNgramsMapping {
        static UNIGRAM_MODELS_FIXTURE: LanguageToNgramsMappingCell = OnceCell::new();
        UNIGRAM_MODELS_FIXTURE.get_or_init(|| {
            hashmap!(
                English => unigram_language_model_for_english,
                German => unigram_language_model_for_german
            )
        })
    }

    #[fixture]
    fn bigram_language_models(
        bigram_language_model_for_english: LazyTrainingDataLanguageModel,
        bigram_language_model_for_german: LazyTrainingDataLanguageModel,
    ) -> LazyLanguageToNgramsMapping {
        static BIGRAM_MODELS_FIXTURE: LanguageToNgramsMappingCell = OnceCell::new();
        BIGRAM_MODELS_FIXTURE.get_or_init(|| {
            hashmap!(
                English => bigram_language_model_for_english,
                German => bigram_language_model_for_german
            )
        })
    }

    #[fixture]
    fn trigram_language_models(
        trigram_language_model_for_english: LazyTrainingDataLanguageModel,
        trigram_language_model_for_german: LazyTrainingDataLanguageModel,
    ) -> LazyLanguageToNgramsMapping {
        static TRIGRAM_MODELS_FIXTURE: LanguageToNgramsMappingCell = OnceCell::new();
        TRIGRAM_MODELS_FIXTURE.get_or_init(|| {
            hashmap!(
                English => trigram_language_model_for_english,
                German => trigram_language_model_for_german
            )
        })
    }

    #[fixture]
    fn quadrigram_language_models(
        quadrigram_language_model_for_english: LazyTrainingDataLanguageModel,
        quadrigram_language_model_for_german: LazyTrainingDataLanguageModel,
    ) -> LazyLanguageToNgramsMapping {
        static QUADRIGRAM_MODELS_FIXTURE: LanguageToNgramsMappingCell = OnceCell::new();
        QUADRIGRAM_MODELS_FIXTURE.get_or_init(|| {
            hashmap!(
                English => quadrigram_language_model_for_english,
                German => quadrigram_language_model_for_german
            )
        })
    }

    #[fixture]
    fn fivegram_language_models(
        fivegram_language_model_for_english: LazyTrainingDataLanguageModel,
        fivegram_language_model_for_german: LazyTrainingDataLanguageModel,
    ) -> LazyLanguageToNgramsMapping {
        static FIVEGRAM_MODELS_FIXTURE: LanguageToNgramsMappingCell = OnceCell::new();
        FIVEGRAM_MODELS_FIXTURE.get_or_init(|| {
            hashmap!(
                English => fivegram_language_model_for_english,
                German => fivegram_language_model_for_german
            )
        })
    }

    // ##############################
    // EMPTY NGRAM MODELS
    // ##############################

    #[fixture]
    fn empty_language_models() -> LazyLanguageToNgramsMapping {
        static EMPTY_MODELS_FIXTURE: LanguageToNgramsMappingCell = OnceCell::new();
        EMPTY_MODELS_FIXTURE.get_or_init(|| hashmap!())
    }

    // ##############################
    // TEST DATA MODELS
    // ##############################

    #[fixture(strs=hashset!())]
    fn test_data_model(strs: HashSet<&'static str>) -> TestDataLanguageModel {
        let ngrams = strs
            .iter()
            .map(|&it| Ngram::new(it))
            .collect::<HashSet<_>>();

        TestDataLanguageModel { ngrams }
    }

    // ##############################
    // DETECTORS
    // ##############################

    #[fixture]
    fn detector_for_english_and_german(
        unigram_language_models: LazyLanguageToNgramsMapping,
        bigram_language_models: LazyLanguageToNgramsMapping,
        trigram_language_models: LazyLanguageToNgramsMapping,
        quadrigram_language_models: LazyLanguageToNgramsMapping,
        fivegram_language_models: LazyLanguageToNgramsMapping,
    ) -> LanguageDetector {
        LanguageDetector {
            languages: hashset!(English, German),
            minimum_relative_distance: 0.0,
            languages_with_unique_characters: hashset!(),
            one_language_alphabets: hashmap!(),
            unigram_language_models,
            bigram_language_models,
            trigram_language_models,
            quadrigram_language_models,
            fivegram_language_models,
        }
    }

    #[fixture]
    fn detector_for_all_languages(
        empty_language_models: LazyLanguageToNgramsMapping,
    ) -> LanguageDetector {
        let languages = Language::all();
        let languages_with_unique_characters = languages
            .iter()
            .filter(|it| it.unique_characters().is_some())
            .cloned()
            .collect();

        let one_language_alphabets = Alphabet::all_supporting_single_language()
            .into_iter()
            .filter(|(_, language)| languages.contains(language))
            .collect();

        LanguageDetector {
            languages,
            minimum_relative_distance: 0.0,
            languages_with_unique_characters,
            one_language_alphabets,
            unigram_language_models: empty_language_models,
            bigram_language_models: empty_language_models,
            trigram_language_models: empty_language_models,
            quadrigram_language_models: empty_language_models,
            fivegram_language_models: empty_language_models,
        }
    }

    // ##############################
    // TESTS
    // ##############################

    #[rstest]
    fn assert_text_is_cleaned_up_properly(detector_for_all_languages: LanguageDetector) {
        let text = "Weltweit    gibt es ungefähr 6.000 Sprachen,
        wobei laut Schätzungen zufolge ungefähr 90  Prozent davon
        am Ende dieses Jahrhunderts verdrängt sein werden.";

        let expected_cleaned_text =
            "weltweit gibt es ungefähr sprachen wobei laut schätzungen zufolge ungefähr \
            prozent davon am ende dieses jahrhunderts verdrängt sein werden";

        assert_eq!(
            detector_for_all_languages.clean_up_input_text(text.to_string()),
            expected_cleaned_text
        );
    }

    #[rstest]
    fn assert_text_is_split_into_words_correctly(detector_for_all_languages: LanguageDetector) {
        assert_eq!(
            detector_for_all_languages.split_text_into_words("this is a sentence"),
            vec!["this", "is", "a", "sentence"]
        );
        assert_eq!(
            detector_for_all_languages.split_text_into_words("sentence"),
            vec!["sentence"]
        );
    }

    #[rstest(
        language,
        ngram,
        expected_probability,
        case(English, "a", 0.01),
        case(English, "lt", 0.12),
        case(English, "ter", 0.21),
        case(English, "alte", 0.25),
        case(English, "alter", 0.29),
        case(German, "t", 0.08),
        case(German, "er", 0.18),
        case(German, "alt", 0.22),
        case(German, "lter", 0.28),
        case(German, "alter", 0.3)
    )]
    fn assert_ngram_probability_lookup_works_correctly(
        detector_for_english_and_german: LanguageDetector,
        language: Language,
        ngram: &str,
        expected_probability: f64,
    ) {
        let probability = detector_for_english_and_german
            .look_up_ngram_probability(&language, &Ngram::new(ngram));
        assert_eq!(
            probability, expected_probability,
            "expected probability {} for language '{:?}' and ngram '{}', got {}",
            expected_probability, language, ngram, probability
        );
    }

    #[rstest]
    #[should_panic(expected = "zerogram detected")]
    fn assert_ngram_probability_lookup_does_not_work_for_zerogram(
        detector_for_english_and_german: LanguageDetector,
    ) {
        detector_for_english_and_german.look_up_ngram_probability(&English, &Ngram::new(""));
    }

    #[rstest(
        ngrams, expected_sum_of_probabilities,
        case(
            hashset!("a", "l", "t", "e", "r"),
            0.01_f64.ln() + 0.02_f64.ln() + 0.03_f64.ln() + 0.04_f64.ln() + 0.05_f64.ln()
        ),
        case(
            // back off unknown Trigram("tez") to known Bigram("te")
            hashset!("alt", "lte", "tez"),
            0.19_f64.ln() + 0.2_f64.ln() + 0.13_f64.ln()
        ),
        case(
            // back off unknown Fivegram("aquas") to known Unigram("a")
            hashset!("aquas"),
            0.01_f64.ln()
        )
    )]
    fn assert_summation_of_ngram_probabilities_works_correctly(
        detector_for_english_and_german: LanguageDetector,
        ngrams: HashSet<&str>,
        expected_sum_of_probabilities: f64,
    ) {
        let mapped_ngrams = ngrams
            .iter()
            .map(|&it| Ngram::new(it))
            .collect::<HashSet<_>>();

        let sum_of_probabilities = detector_for_english_and_german
            .compute_sum_of_ngram_probabilities(&English, &mapped_ngrams);

        assert!(
            approx_eq!(
                f64,
                sum_of_probabilities,
                expected_sum_of_probabilities,
                ulps = 1
            ),
            "expected sum {} for language '{:?}' and ngrams {:?}, got {}",
            expected_sum_of_probabilities,
            English,
            ngrams,
            sum_of_probabilities
        );
    }

    #[rstest(
        test_data_model,
        expected_probabilities,
        case::unigram_model(
            test_data_model(hashset!("a", "l", "t", "e", "r")),
            hashmap!(
                English => 0.01_f64.ln() + 0.02_f64.ln() + 0.03_f64.ln() + 0.04_f64.ln() + 0.05_f64.ln(),
                German => 0.06_f64.ln() + 0.07_f64.ln() + 0.08_f64.ln() + 0.09_f64.ln() + 0.1_f64.ln()
            )
        ),
        case::trigram_model(
            test_data_model(hashset!("alt", "lte", "ter", "wxy")),
            hashmap!(
                English => 0.19_f64.ln() + 0.2_f64.ln() + 0.21_f64.ln(),
                German => 0.22_f64.ln() + 0.23_f64.ln() + 0.24_f64.ln()
            )
        ),
        case::quadrigram_model(
            test_data_model(hashset!("alte", "lter", "wxyz")),
            hashmap!(
                English => 0.25_f64.ln() + 0.26_f64.ln(),
                German => 0.27_f64.ln() + 0.28_f64.ln()
            )
        )
    )]
    fn assert_computation_of_language_probabilities_works_correctly(
        detector_for_english_and_german: LanguageDetector,
        test_data_model: TestDataLanguageModel,
        expected_probabilities: HashMap<Language, f64>,
    ) {
        let probabilities = detector_for_english_and_german
            .compute_language_probabilities(&test_data_model, &hashset!(English, German));

        for (language, probability) in probabilities {
            let expected_probability = expected_probabilities[&language];

            assert!(
                approx_eq!(f64, probability, expected_probability, ulps = 1),
                "expected probability {} for language '{:?}', got {}",
                expected_probability,
                language,
                probability
            );
        }
    }

    #[rstest]
    fn assert_computation_of_confidence_values_works_correctly(
        detector_for_english_and_german: LanguageDetector,
    ) {
        let unigram_count_for_both_languages = 5.0;

        let total_probability_for_german = (
            // unigrams
            0.06_f64.ln() + 0.07_f64.ln() + 0.08_f64.ln() + 0.09_f64.ln() + 0.1_f64.ln() +
            // bigrams
            0.15_f64.ln() + 0.16_f64.ln() + 0.17_f64.ln() + 0.18_f64.ln() +
            // trigrams
            0.22_f64.ln() + 0.23_f64.ln() + 0.24_f64.ln() +
            // quadrigrams
            0.27_f64.ln() + 0.28_f64.ln() +
            // fivegrams
            0.3_f64.ln()
        ) / unigram_count_for_both_languages;

        let total_probability_for_english = (
            // unigrams
            0.01_f64.ln() + 0.02_f64.ln() + 0.03_f64.ln() + 0.04_f64.ln() + 0.05_f64.ln() +
            // bigrams
            0.11_f64.ln() + 0.12_f64.ln() + 0.13_f64.ln() + 0.14_f64.ln() +
            // trigrams
            0.19_f64.ln() + 0.2_f64.ln() + 0.21_f64.ln() +
            // quadrigrams
            0.25_f64.ln() + 0.26_f64.ln() +
            // fivegrams
            0.29_f64.ln()
        ) / unigram_count_for_both_languages;

        let expected_confidence_for_german = 1.0;
        let expected_confidence_for_english =
            total_probability_for_german / total_probability_for_english;

        let confidence_values =
            detector_for_english_and_german.compute_language_confidence_values("Alter");

        assert_eq!(
            confidence_values[0],
            (German, expected_confidence_for_german)
        );
        assert_eq!(confidence_values[1].0, English);
        assert!(approx_eq!(
            f64,
            confidence_values[1].1,
            expected_confidence_for_english,
            ulps = 1
        ));
    }

    #[rstest]
    fn assert_language_of_german_noun_alter_is_detected_correctly(
        detector_for_english_and_german: LanguageDetector,
    ) {
        let detected_language = detector_for_english_and_german.detect_language_of("Alter");
        assert_eq!(detected_language, Some(German));
    }

    #[rstest]
    fn assert_no_language_is_returned_when_no_ngram_probabilities_are_available(
        detector_for_english_and_german: LanguageDetector,
    ) {
        let detected_language = detector_for_english_and_german.detect_language_of("проарплап");
        assert_eq!(detected_language, None);
    }

    #[rstest]
    fn assert_no_confidence_values_are_returned_when_no_ngram_probabilities_are_available(
        detector_for_english_and_german: LanguageDetector,
    ) {
        let confidence_values =
            detector_for_english_and_german.compute_language_confidence_values("проарплап");

        assert_eq!(confidence_values, vec![]);
    }

    #[rstest(
        word,
        expected_language,
        // words with unique characters
        case("məhərrəm", Some(Azerbaijani)),
        case("substituïts", Some(Catalan)),
        case("rozdělit", Some(Czech)),
        case("tvořen", Some(Czech)),
        case("subjektů", Some(Czech)),
        case("nesufiĉecon", Some(Esperanto)),
        case("intermiksiĝis", Some(Esperanto)),
        case("monaĥinoj", Some(Esperanto)),
        case("kreitaĵoj", Some(Esperanto)),
        case("ŝpinante", Some(Esperanto)),
        case("apenaŭ", Some(Esperanto)),
        case("groß", Some(German)),
        case("σχέδια", Some(Greek)),
        case("fekvő", Some(Hungarian)),
        case("meggyűrűzni", Some(Hungarian)),
        case("ヴェダイヤモンド", Some(Japanese)),
        case("әлем", Some(Kazakh)),
        case("шаруашылығы", Some(Kazakh)),
        case("ақын", Some(Kazakh)),
        case("оның", Some(Kazakh)),
        case("шұрайлы", Some(Kazakh)),
        case("teoloģiska", Some(Latvian)),
        case("blaķene", Some(Latvian)),
        case("ceļojumiem", Some(Latvian)),
        case("numuriņu", Some(Latvian)),
        case("mergelės", Some(Lithuanian)),
        case("įrengus", Some(Lithuanian)),
        case("slegiamų", Some(Lithuanian)),
        case("припаѓа", Some(Macedonian)),
        case("ѕидови", Some(Macedonian)),
        case("ќерка", Some(Macedonian)),
        case("џамиите", Some(Macedonian)),
        case("मिळते", Some(Marathi)),
        case("үндсэн", Some(Mongolian)),
        case("дөхөж", Some(Mongolian)),
        case("zmieniły", Some(Polish)),
        case("państwowych", Some(Polish)),
        case("mniejszości", Some(Polish)),
        case("groźne", Some(Polish)),
        case("ialomiţa", Some(Romanian)),
        case("наслеђивања", Some(Serbian)),
        case("неисквареношћу", Some(Serbian)),
        case("podĺa", Some(Slovak)),
        case("pohľade", Some(Slovak)),
        case("mŕtvych", Some(Slovak)),
        case("ґрунтовому", Some(Ukrainian)),
        case("пропонує", Some(Ukrainian)),
        case("пристрої", Some(Ukrainian)),
        case("cằm", Some(Vietnamese)),
        case("thần", Some(Vietnamese)),
        case("chẳng", Some(Vietnamese)),
        case("quẩy", Some(Vietnamese)),
        case("sẵn", Some(Vietnamese)),
        case("nhẫn", Some(Vietnamese)),
        case("dắt", Some(Vietnamese)),
        case("chất", Some(Vietnamese)),
        case("đạp", Some(Vietnamese)),
        case("mặn", Some(Vietnamese)),
        case("hậu", Some(Vietnamese)),
        case("hiền", Some(Vietnamese)),
        case("lẻn", Some(Vietnamese)),
        case("biểu", Some(Vietnamese)),
        case("kẽm", Some(Vietnamese)),
        case("diễm", Some(Vietnamese)),
        case("phế", Some(Vietnamese)),
        case("việc", Some(Vietnamese)),
        case("chỉnh", Some(Vietnamese)),
        case("trĩ", Some(Vietnamese)),
        case("ravị", Some(Vietnamese)),
        case("thơ", Some(Vietnamese)),
        case("nguồn", Some(Vietnamese)),
        case("thờ", Some(Vietnamese)),
        case("sỏi", Some(Vietnamese)),
        case("tổng", Some(Vietnamese)),
        case("nhở", Some(Vietnamese)),
        case("mỗi", Some(Vietnamese)),
        case("bỡi", Some(Vietnamese)),
        case("tốt", Some(Vietnamese)),
        case("giới", Some(Vietnamese)),
        case("một", Some(Vietnamese)),
        case("hợp", Some(Vietnamese)),
        case("hưng", Some(Vietnamese)),
        case("từng", Some(Vietnamese)),
        case("của", Some(Vietnamese)),
        case("sử", Some(Vietnamese)),
        case("cũng", Some(Vietnamese)),
        case("những", Some(Vietnamese)),
        case("chức", Some(Vietnamese)),
        case("dụng", Some(Vietnamese)),
        case("thực", Some(Vietnamese)),
        case("kỳ", Some(Vietnamese)),
        case("kỷ", Some(Vietnamese)),
        case("mỹ", Some(Vietnamese)),
        case("mỵ", Some(Vietnamese)),
        case("aṣiwèrè", Some(Yoruba)),
        case("ṣaaju", Some(Yoruba)),
        case("والموضوع", None),
        case("сопротивление", None),
        case("house", None),

        // words with unique alphabet
        case("ունենա", Some(Armenian)),
        case("জানাতে", Some(Bengali)),
        case("გარეუბან", Some(Georgian)),
        case("σταμάτησε", Some(Greek)),
        case("ઉપકરણોની", Some(Gujarati)),
        case("בתחרויות", Some(Hebrew)),
        case("びさ", Some(Japanese)),
        case("대결구도가", Some(Korean)),
        case("ਮੋਟਰਸਾਈਕਲਾਂ", Some(Punjabi)),
        case("துன்பங்களை", Some(Tamil)),
        case("కృష్ణదేవరాయలు", Some(Telugu)),
        case("ในทางหลวงหมายเลข", Some(Thai)),
    )]
    fn assert_language_detection_with_rules_works_correctly(
        detector_for_all_languages: LanguageDetector,
        word: &str,
        expected_language: Option<Language>,
    ) {
        let detected_language = detector_for_all_languages.detect_language_with_rules(&vec![word]);
        assert_eq!(
            detected_language, expected_language,
            "expected {:?} for word '{}', got {:?}",
            expected_language, word, detected_language
        );
    }

    #[rstest(word, expected_languages,
        case("والموضوع", hashset!(Arabic, Persian, Urdu)),
        case(
            "сопротивление",
            hashset!(
                Belarusian, Bulgarian, Kazakh, Macedonian, Mongolian, Russian, Serbian, Ukrainian
            )
        ),
        case("раскрывае", hashset!(Belarusian, Kazakh, Mongolian, Russian)),
        case("этот", hashset!(Belarusian, Kazakh, Mongolian, Russian)),
        case("огнём", hashset!(Belarusian, Kazakh, Mongolian, Russian)),
        case("плаваща", hashset!(Bulgarian, Kazakh, Mongolian, Russian)),
        case("довършат", hashset!(Bulgarian, Kazakh, Mongolian, Russian)),
        case("павінен", hashset!(Belarusian, Kazakh, Ukrainian)),
        case("затоплување", hashset!(Macedonian, Serbian)),
        case("ректасцензија", hashset!(Macedonian, Serbian)),
        case("набљудувач", hashset!(Macedonian, Serbian)),
        case("aizklātā", hashset!(Latvian, Maori, Yoruba)),
        case("sistēmas", hashset!(Latvian, Maori, Yoruba)),
        case("palīdzi", hashset!(Latvian, Maori, Yoruba)),
        case("nhẹn", hashset!(Vietnamese, Yoruba)),
        case("chọn", hashset!(Vietnamese, Yoruba)),
        case("prihvaćanju", hashset!(Bosnian, Croatian, Polish)),
        case("nađete", hashset!(Bosnian, Croatian, Vietnamese)),
        case("visão", hashset!(Portuguese, Vietnamese)),
        case("wystąpią", hashset!(Lithuanian, Polish)),
        case("budowę", hashset!(Lithuanian, Polish)),
        case("nebūsime", hashset!(Latvian, Lithuanian, Maori, Yoruba)),
        case("afişate", hashset!(Azerbaijani, Romanian, Turkish)),
        case("kradzieżami", hashset!(Polish, Romanian)),
        case("înviat", hashset!(French, Romanian)),
        case("venerdì", hashset!(Italian, Vietnamese, Yoruba)),
        case("años", hashset!(Basque, Spanish)),
        case("rozohňuje", hashset!(Czech, Slovak)),
        case("rtuť", hashset!(Czech, Slovak)),
        case("pregătire", hashset!(Romanian, Vietnamese)),
        case("jeďte", hashset!(Czech, Romanian, Slovak)),
        case("minjaverðir", hashset!(Icelandic, Latvian, Turkish)),
        case("þagnarskyldu", hashset!(Icelandic, Latvian, Turkish)),
        case("nebûtu", hashset!(French, Hungarian, Latvian)),
        case("hashemidëve", hashset!(Afrikaans, Albanian, Dutch, French)),
        case("forêt", hashset!(Afrikaans, French, Portuguese, Vietnamese)),
        case("succèdent", hashset!(French, Italian, Vietnamese, Yoruba)),
        case("où", hashset!(French, Italian, Vietnamese, Yoruba)),
        case("tõeliseks", hashset!(Estonian, Hungarian, Portuguese, Vietnamese)),
        case("viòiem", hashset!(Catalan, Italian, Latvian, Vietnamese, Yoruba)),
        case("contrôle", hashset!(French, Portuguese, Slovak, Vietnamese)),
        case("direktør", hashset!(Bokmal, Danish, Nynorsk)),
        case("vývoj", hashset!(Czech, Icelandic, Slovak, Turkish, Vietnamese)),
        case("päralt", hashset!(Estonian, Finnish, German, Slovak, Swedish)),
        case("labâk", hashset!(Latvian, Portuguese, Romanian, Turkish, Vietnamese)),
        case("pràctiques", hashset!(Catalan, French, Italian, Portuguese, Vietnamese)),
        case(
            "überrascht",
            hashset!(Azerbaijani, Catalan, Estonian, German, Hungarian, Spanish, Turkish)
        ),
        case("indebærer", hashset!(Bokmal, Danish, Icelandic, Nynorsk)),
        case("måned", hashset!(Bokmal, Danish, Nynorsk, Swedish)),
        case("zaručen", hashset!(Bosnian, Czech, Croatian, Latvian, Lithuanian, Slovak, Slovene)),
        case("zkouškou", hashset!(Bosnian, Czech, Croatian, Latvian, Lithuanian, Slovak, Slovene)),
        case("navržen", hashset!(Bosnian, Czech, Croatian, Latvian, Lithuanian, Slovak, Slovene)),
        case(
            "façonnage",
            hashset!(Albanian, Azerbaijani, Basque, Catalan, French, Latvian, Portuguese, Turkish)
        ),
        case(
            "höher",
            hashset!(Azerbaijani, Estonian, Finnish, German, Hungarian, Icelandic, Swedish, Turkish)
        ),
        case(
            "catedráticos",
            hashset!(
                Catalan, Czech, Icelandic, Irish, Hungarian, Portuguese, Slovak, Spanish,
                Vietnamese, Yoruba
            )
        ),
        case(
            "política",
            hashset!(
                Catalan, Czech, Icelandic, Irish, Hungarian, Portuguese, Slovak, Spanish,
                Vietnamese, Yoruba
            )
        ),
        case(
            "música",
            hashset!(
                Catalan, Czech, Icelandic, Irish, Hungarian, Portuguese, Slovak, Spanish,
                Vietnamese, Yoruba
            )
        ),
        case(
            "contradicció",
            hashset!(
                Catalan, Hungarian, Icelandic, Irish, Polish, Portuguese, Slovak, Spanish,
                Vietnamese, Yoruba
            )
        ),
        case(
            "només",
            hashset!(
                Catalan, Czech, French, Hungarian, Icelandic, Irish, Italian, Portuguese, Slovak,
                Spanish, Vietnamese, Yoruba
            )
        ),
        case(
            "house",
            hashset!(
                Afrikaans, Albanian, Azerbaijani, Basque, Bokmal, Bosnian, Catalan, Croatian, Czech,
                Danish, Dutch, English, Esperanto, Estonian, Finnish, French, Ganda, German, Hungarian,
                Icelandic, Indonesian, Irish, Italian, Latin, Latvian, Lithuanian, Malay, Maori, Nynorsk,
                Polish, Portuguese, Romanian, Shona, Slovak, Slovene, Somali, Sotho, Spanish, Swahili,
                Swedish, Tagalog, Tsonga, Tswana, Turkish, Vietnamese, Welsh, Xhosa, Yoruba, Zulu
            )
        ),
    )]
    fn assert_language_filtering_with_rules_works_correctly(
        detector_for_all_languages: LanguageDetector,
        word: &str,
        expected_languages: HashSet<Language>,
    ) {
        let filtered_languages = detector_for_all_languages.filter_languages_by_rules(vec![word]);
        assert_eq!(
            filtered_languages, expected_languages,
            "expected {:?} for word '{}', got {:?}",
            expected_languages, word, filtered_languages
        );
    }

    #[rstest(invalid_str, case(""), case(" \n  \t;"), case("3<856%)§"))]
    fn assert_strings_without_letters_return_no_language(
        detector_for_all_languages: LanguageDetector,
        invalid_str: &str,
    ) {
        assert_eq!(
            detector_for_all_languages.detect_language_of(invalid_str),
            None
        );
    }
}
