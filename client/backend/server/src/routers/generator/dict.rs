//! Dictionary loading + scoring — port of `loadDictionary` /
//! `loadWordNetFrequencyScores` / `scoreWordQuality` from generateCrossword.ts.
//! Split into an async DB fetch (`fetch_rows`) and a CPU-bound `build_dictionary`
//! (runs the ONNX embedding) so the orchestrator can `spawn_blocking` the heavy
//! part off the tokio runtime.

use super::embed::{self, Candidate};
use super::solver::Params;
use serde_json::{json, Value};
use sqlx::{PgPool, Row};
use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;

const WORDNET_COUNT_PATH: &str = "data/crossword/wordnet/dict/cntlist";

pub struct Dictionary {
    pub dictionary_set: HashSet<String>,
    pub topic_words: HashSet<String>,
    pub topic_scores: HashMap<String, f64>,
    pub quality_scores: HashMap<String, i32>,
    pub frequency_scores: HashMap<String, f64>,
    pub clue_by_word: HashMap<String, String>,
    pub by_letter: HashMap<u8, Vec<String>>,
    pub by_length: HashMap<usize, Vec<String>>,
}

#[derive(Clone)]
pub struct Def {
    pub pos: String,
    pub gloss: String,
}

pub struct RawRow {
    pub word: String,
    pub defs: Vec<Def>,
}

/// Async: pull dictionary words (with up to 8 definitions each) in the length
/// range. Owned + Send so the caller can move it into `spawn_blocking`.
pub async fn fetch_rows(pool: &PgPool, p: &Params) -> Result<Vec<RawRow>, String> {
    let rows = sqlx::query(
        r#"
        SELECT w.word,
          COALESCE((
            SELECT json_agg(json_build_object('pos', d."partOfSpeech"::text, 'gloss', d.gloss))
            FROM (
              SELECT dd."partOfSpeech", dd.gloss
              FROM "DictionaryDefinition" dd WHERE dd."wordId" = w.id LIMIT 8
            ) d
          ), '[]'::json) AS defs
        FROM "DictionaryWord" w
        WHERE w.length BETWEEN $1 AND $2
          AND EXISTS (SELECT 1 FROM "DictionaryDefinition" dd WHERE dd."wordId" = w.id)
        ORDER BY w.length DESC, w.word ASC
        "#,
    )
    .bind(p.min_len)
    .bind(p.max_len)
    .fetch_all(pool)
    .await
    .map_err(|e| e.to_string())?;

    Ok(rows
        .into_iter()
        .map(|r| {
            let word: String = r.get("word");
            let defs_json: Value = r.get("defs");
            let defs = defs_json
                .as_array()
                .map(|arr| {
                    arr.iter()
                        .map(|d| Def {
                            pos: d["pos"].as_str().unwrap_or("").to_string(),
                            gloss: d["gloss"].as_str().unwrap_or("").to_string(),
                        })
                        .collect()
                })
                .unwrap_or_default();
            RawRow { word, defs }
        })
        .collect())
}

/// Read WordNet `cntlist` (`tag_cnt sense_key sense_number`) into a per-lemma
/// frequency total, mirroring `loadWordNetFrequencyScores`. Missing file => empty.
fn load_frequency_scores() -> HashMap<String, f64> {
    let mut scores: HashMap<String, f64> = HashMap::new();
    let Ok(contents) = std::fs::read_to_string(WORDNET_COUNT_PATH) else {
        return scores;
    };
    for line in contents.lines() {
        let t = line.trim();
        if t.is_empty() {
            continue;
        }
        let mut parts = t.split_whitespace();
        let (Some(count_text), Some(sense_key)) = (parts.next(), parts.next()) else {
            continue;
        };
        let Ok(count) = count_text.parse::<f64>() else {
            continue;
        };
        let lemma = sense_key.split('%').next().unwrap_or("");
        if lemma.is_empty()
            || lemma.contains('_')
            || lemma.contains('-')
            || lemma.contains('\'')
        {
            continue;
        }
        let word = lemma.to_uppercase();
        if !word.bytes().all(|b| b.is_ascii_uppercase()) {
            continue;
        }
        *scores.entry(word).or_insert(0.0) += count;
    }
    scores
}

fn clean_clue(gloss: &str) -> String {
    let trimmed = gloss.trim().trim_matches(['"', '\'']);
    trimmed.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn bad_gloss_patterns() -> &'static [regex::Regex] {
    static PATTERNS: OnceLock<Vec<regex::Regex>> = OnceLock::new();
    PATTERNS.get_or_init(|| {
        [
            r"(?i)\babbreviation\b",
            r"(?i)\bacronym\b",
            r"(?i)\bRoman numeral\b",
            r"(?i)\bunit of measurement\b",
            r"(?i)\bgenus\b",
            r"(?i)\bfamily\b",
            r"(?i)\btaxonomic\b",
            r"(?i)\bpidgin\b",
            r"(?i)\bvariety of zircon\b",
            r"(?i)\barchaic\b",
            r"(?i)\bobsolete\b",
        ]
        .iter()
        .map(|p| regex::Regex::new(p).unwrap())
        .collect()
    })
}

/// Port of `scoreWordQuality`. Assumes `word` is uppercase ASCII (as seeded).
fn score_word_quality(word: &str, defs: &[Def], frequency_score: f64) -> i32 {
    let gloss_text = defs
        .iter()
        .map(|d| d.gloss.as_str())
        .collect::<Vec<_>>()
        .join(" ");
    let pos: HashSet<&str> = defs.iter().map(|d| d.pos.as_str()).collect();
    let mut score = 0i32;

    let has_vowel = word.bytes().any(|b| b"AEIOUY".contains(&b));
    if !has_vowel {
        return -10;
    }
    if word.len() <= 5 && frequency_score < 5.0 {
        return -10;
    }
    if has_triple_letter(word) {
        score -= 2;
    }
    if word.bytes().any(|b| b"QXZ".contains(&b)) {
        score -= 1;
    }
    if word.len() >= 4 && word.len() <= 8 {
        score += 2;
    }
    if word.len() == 3 {
        score -= 1;
    }
    if word.len() >= 9 {
        score -= 1;
    }
    if frequency_score >= 50.0 {
        score += 5;
    } else if frequency_score >= 20.0 {
        score += 4;
    } else if frequency_score >= 10.0 {
        score += 3;
    } else if frequency_score >= 5.0 {
        score += 2;
    }
    if pos.contains("NOUN") {
        score += 1;
    }
    if pos.contains("VERB") {
        score += 1;
    }
    if pos.contains("ADJECTIVE") {
        score += 1;
    }
    if defs.len() > 1 {
        score += 1;
    }
    if bad_gloss_patterns().iter().any(|re| re.is_match(&gloss_text)) {
        score -= 6;
    }
    if word.len() <= 3 && word.bytes().all(|b| b.is_ascii_uppercase()) {
        score -= 4;
    }
    if !word.is_empty() && word.bytes().all(|b| b"XVI".contains(&b)) {
        score -= 8;
    }
    score
}

fn has_triple_letter(word: &str) -> bool {
    word.as_bytes().windows(3).any(|w| w[0] == w[1] && w[1] == w[2])
}

fn candidate_embed_text(word: &str, defs: &[Def]) -> String {
    let glosses = defs
        .iter()
        .take(4)
        .map(|d| d.gloss.as_str())
        .collect::<Vec<_>>()
        .join("; ");
    format!("{}: {}", word.to_lowercase(), glosses)
}

/// CPU-bound: score quality, run the embedding model for topic relevance, and
/// assemble the indexes the solver needs. Port of `loadDictionary`'s body.
pub fn build_dictionary(
    rows: Vec<RawRow>,
    topic: &str,
    emit: &mut dyn FnMut(Value),
) -> Result<Dictionary, String> {
    let frequency = load_frequency_scores();

    struct Scored {
        word: String,
        defs: Vec<Def>,
        frequency: f64,
        quality: i32,
    }
    let total_rows = rows.len();
    let scored: Vec<Scored> = rows
        .into_iter()
        .map(|r| {
            let frequency = frequency.get(&r.word).copied().unwrap_or(0.0);
            let quality = score_word_quality(&r.word, &r.defs, frequency);
            Scored {
                word: r.word,
                defs: r.defs,
                frequency,
                quality,
            }
        })
        .filter(|s| s.quality >= 3)
        .collect();

    if scored.is_empty() {
        return Err("Dictionary is empty. Seed DictionaryWord first.".to_string());
    }

    emit(json!({
        "type": "log", "level": "info",
        "message": format!("{} dictionary rows in range; {} passed the quality filter", total_rows, scored.len()),
    }));

    let candidates: Vec<Candidate> = scored
        .iter()
        .map(|s| Candidate {
            word: s.word.clone(),
            embed_text: candidate_embed_text(&s.word, &s.defs),
            quality: s.quality,
        })
        .collect();
    let topic_scores = embed::score_candidates(topic, candidates, emit)?;

    // top 800 topic words
    let mut by_topic: Vec<(&String, f64)> = topic_scores.iter().map(|(w, s)| (w, *s)).collect();
    by_topic.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    let topic_words: HashSet<String> = by_topic
        .iter()
        .take(800)
        .map(|(w, _)| (*w).clone())
        .collect();

    let quality_scores: HashMap<String, i32> =
        scored.iter().map(|s| (s.word.clone(), s.quality)).collect();
    let frequency_scores: HashMap<String, f64> =
        scored.iter().map(|s| (s.word.clone(), s.frequency)).collect();
    let clue_by_word: HashMap<String, String> = scored
        .iter()
        .map(|s| {
            let gloss = s.defs.first().map(|d| d.gloss.as_str()).unwrap_or(&s.word);
            (s.word.clone(), clean_clue(gloss))
        })
        .collect();
    let dictionary_set: HashSet<String> = scored.iter().map(|s| s.word.clone()).collect();

    let mut by_letter: HashMap<u8, Vec<String>> = HashMap::new();
    let mut by_length: HashMap<usize, Vec<String>> = HashMap::new();
    for s in &scored {
        by_length.entry(s.word.len()).or_default().push(s.word.clone());
        for b in s.word.bytes().collect::<HashSet<_>>() {
            by_letter.entry(b).or_default().push(s.word.clone());
        }
    }

    // sort each letter bucket by topic, then frequency, then quality, then length; cap 2500
    for words in by_letter.values_mut() {
        words.sort_by(|a, b| {
            let ta = topic_scores.get(a).copied().unwrap_or(0.0);
            let tb = topic_scores.get(b).copied().unwrap_or(0.0);
            tb.partial_cmp(&ta)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| {
                    let fa = frequency_scores.get(a).copied().unwrap_or(0.0);
                    let fb = frequency_scores.get(b).copied().unwrap_or(0.0);
                    fb.partial_cmp(&fa).unwrap_or(std::cmp::Ordering::Equal)
                })
                .then_with(|| {
                    let qa = quality_scores.get(a).copied().unwrap_or(0);
                    let qb = quality_scores.get(b).copied().unwrap_or(0);
                    qb.cmp(&qa)
                })
                .then_with(|| a.len().cmp(&b.len()))
        });
        words.truncate(2500);
    }

    Ok(Dictionary {
        dictionary_set,
        topic_words,
        topic_scores,
        quality_scores,
        frequency_scores,
        clue_by_word,
        by_letter,
        by_length,
    })
}
