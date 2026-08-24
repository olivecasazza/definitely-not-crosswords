//! `seed` — faithful port of scripts/seed_wordnet_dictionary.mjs.
//!
//! Parses the WordNet `data.*` files, derives unique words + definitions, and
//! bulk-inserts them into "DictionaryWord" / "DictionaryDefinition" with
//! ON CONFLICT DO NOTHING. Flags: `--dry-run` (parse + report, no DB writes),
//! `--skip-if-present` (skip if existing WORDNET counts already cover the parse).
//!
//! Data directory is read from WORDNET_DICT_DIR (default
//! "data/crossword/wordnet/dict", relative to the current working directory —
//! same default as the original .mjs).

use std::collections::HashMap;
use std::path::Path;

use anyhow::Context;
use serde_json::{json, Value};
use sqlx::postgres::PgPoolOptions;

const DEFAULT_DICT_DIR: &str = "data/crossword/wordnet/dict";
const MIN_WORD_LENGTH: usize = 2;
const MAX_WORD_LENGTH: usize = 50;
const WORD_CHUNK_SIZE: usize = 1000;
const DEFINITION_CHUNK_SIZE: usize = 5000;

const DATA_FILES: &[(&str, &str)] = &[
    ("data.noun", "NOUN"),
    ("data.verb", "VERB"),
    ("data.adj", "ADJECTIVE"),
    ("data.adv", "ADVERB"),
];

/// A single parsed (word, synset) row prior to dedup.
struct ParsedRow {
    word: String,
    lemma: String,
    length: i32,
    part_of_speech: &'static str,
    synset_offset: String,
    gloss: String,
    examples: Option<Vec<String>>,
}

/// Reject multi-word / hyphenated / apostrophe lemmas, uppercase, require pure
/// A-Z and length within bounds. Mirrors normalizeLemma() in the .mjs.
fn normalize_lemma(lemma: &str) -> Option<String> {
    if lemma.contains('_') || lemma.contains('-') || lemma.contains('\'') {
        return None;
    }
    let word = lemma.to_uppercase();
    if !word.bytes().all(|b| b.is_ascii_uppercase()) || word.is_empty() {
        return None;
    }
    if word.len() < MIN_WORD_LENGTH || word.len() > MAX_WORD_LENGTH {
        return None;
    }
    Some(word)
}

/// Split a raw gloss on ';' into the definition (first part) and the quoted
/// example sentences. Mirrors parseGloss() in the .mjs.
fn parse_gloss(raw_gloss: &str) -> (String, Option<Vec<String>>) {
    let parts: Vec<&str> = raw_gloss
        .split(';')
        .map(|p| p.trim())
        .filter(|p| !p.is_empty())
        .collect();

    let definition = parts
        .first()
        .map(|s| s.to_string())
        .unwrap_or_else(|| raw_gloss.trim().to_string());

    let examples: Vec<String> = parts
        .iter()
        .skip(1)
        .filter_map(|part| {
            // Match ^"(.+)"$ — strip the surrounding quotes if both present.
            let bytes = part.as_bytes();
            if bytes.len() >= 3 && bytes[0] == b'"' && bytes[bytes.len() - 1] == b'"' {
                Some(part[1..part.len() - 1].to_string())
            } else {
                None
            }
        })
        .collect();

    (
        definition,
        if examples.is_empty() {
            None
        } else {
            Some(examples)
        },
    )
}

fn parse_data_file(path: &Path, part_of_speech: &'static str) -> anyhow::Result<Vec<ParsedRow>> {
    let contents = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read WordNet data file: {}", path.display()))?;

    let mut rows = Vec::new();

    for line in contents.split('\n') {
        let line = line.strip_suffix('\r').unwrap_or(line);
        if line.is_empty() || line.starts_with("  ") {
            continue;
        }

        let (raw_synset, raw_gloss) = match line.split_once(" | ") {
            Some((s, g)) => (s, g),
            None => (line, ""),
        };

        let fields: Vec<&str> = raw_synset.split_whitespace().collect();
        if fields.len() < 5 {
            continue;
        }

        let synset_offset = fields[0].to_string();
        let word_count = match i64::from_str_radix(fields[3], 16) {
            Ok(n) if n >= 1 => n as usize,
            _ => continue,
        };

        let (gloss, examples) = parse_gloss(raw_gloss);
        let first_word_index = 4;

        for word_index in 0..word_count {
            let field_idx = first_word_index + word_index * 2;
            let lemma = match fields.get(field_idx) {
                Some(l) => *l,
                None => continue,
            };

            let word = match normalize_lemma(lemma) {
                Some(w) => w,
                None => continue,
            };

            rows.push(ParsedRow {
                length: word.len() as i32,
                word,
                lemma: lemma.to_string(),
                part_of_speech,
                synset_offset: synset_offset.clone(),
                gloss: gloss.clone(),
                examples: examples.clone(),
            });
        }
    }

    Ok(rows)
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let is_dry_run = args.iter().any(|a| a == "--dry-run");
    let is_skip_if_present = args.iter().any(|a| a == "--skip-if-present");

    let dict_dir =
        std::env::var("WORDNET_DICT_DIR").unwrap_or_else(|_| DEFAULT_DICT_DIR.to_string());
    let dict_dir = Path::new(&dict_dir);

    // Parse all files in order.
    let mut parsed_rows: Vec<ParsedRow> = Vec::new();
    for (file, pos) in DATA_FILES {
        let path = dict_dir.join(file);
        parsed_rows.extend(parse_data_file(&path, pos)?);
    }

    // Unique words by word (first occurrence wins). Preserve insertion order.
    let mut word_order: Vec<String> = Vec::new();
    let mut word_length: HashMap<String, i32> = HashMap::new();
    for row in &parsed_rows {
        if !word_length.contains_key(&row.word) {
            word_length.insert(row.word.clone(), row.length);
            word_order.push(row.word.clone());
        }
    }

    // Unique definitions by (word, partOfSpeech, synsetOffset), first wins.
    let mut def_seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut unique_defs: Vec<&ParsedRow> = Vec::new();
    for row in &parsed_rows {
        let key = format!("{}:{}:{}", row.word, row.part_of_speech, row.synset_offset);
        if def_seen.insert(key) {
            unique_defs.push(row);
        }
    }

    let word_total = word_order.len();
    let unique_def_total = unique_defs.len();

    if is_dry_run {
        println!("Parsed {word_total} WordNet words.");
        println!(
            "Parsed {} WordNet definitions ({} unique).",
            parsed_rows.len(),
            unique_def_total
        );
        println!("Dry run complete; database was not modified.");
        return Ok(());
    }

    let db_url = std::env::var("DATABASE_URL").context("DATABASE_URL env var is not set")?;
    let pool = PgPoolOptions::new()
        .max_connections(4)
        .connect(&db_url)
        .await
        .context("failed to connect to the database")?;

    if is_skip_if_present {
        let word_count: i64 =
            sqlx::query_scalar(r#"SELECT COUNT(*) FROM "DictionaryWord" WHERE source = 'WORDNET'"#)
                .fetch_one(&pool)
                .await?;
        let def_count: i64 = sqlx::query_scalar(
            r#"SELECT COUNT(*) FROM "DictionaryDefinition" WHERE source = 'WORDNET'"#,
        )
        .fetch_one(&pool)
        .await?;

        if word_count >= word_total as i64 && def_count >= unique_def_total as i64 {
            println!("Dictionary already seeded ({word_count} words, {def_count} definitions).");
            pool.close().await;
            return Ok(());
        }
    }

    // --- Words ---
    let existing_word_count: i64 =
        sqlx::query_scalar(r#"SELECT COUNT(*) FROM "DictionaryWord" WHERE source = 'WORDNET'"#)
            .fetch_one(&pool)
            .await?;

    if existing_word_count < word_total as i64 {
        let mut completed = 0usize;
        for chunk in word_order.chunks(WORD_CHUNK_SIZE) {
            let words: Vec<String> = chunk.to_vec();
            let lengths: Vec<i32> = chunk.iter().map(|w| word_length[w]).collect();

            sqlx::query(
                r#"
                INSERT INTO "DictionaryWord" ("id", "word", "length", "source", "updatedAt")
                SELECT gen_random_uuid()::text, w.word, w.length,
                       'WORDNET'::"DictionarySource", now()
                FROM UNNEST($1::text[], $2::int[]) AS w(word, length)
                ON CONFLICT ("word") DO NOTHING
                "#,
            )
            .bind(&words)
            .bind(&lengths)
            .execute(&pool)
            .await
            .context("failed to insert DictionaryWord chunk")?;

            completed += chunk.len();
            println!(
                "WordNet words: {}/{}",
                completed.min(word_total),
                word_total
            );
        }
    } else {
        println!("WordNet words already present ({existing_word_count}/{word_total}).");
    }

    // Map word -> id for all WORDNET words.
    let word_id_rows: Vec<(String, String)> =
        sqlx::query_as(r#"SELECT "word", "id" FROM "DictionaryWord" WHERE source = 'WORDNET'"#)
            .fetch_all(&pool)
            .await?;
    let word_id: HashMap<String, String> = word_id_rows.into_iter().collect();

    // Build definition payload rows (drop any whose word didn't resolve to an id).
    let def_payload: Vec<Value> = unique_defs
        .iter()
        .filter_map(|row| {
            let wid = word_id.get(&row.word)?;
            Some(json!({
                "wordId": wid,
                "partOfSpeech": row.part_of_speech,
                "synsetOffset": row.synset_offset,
                "lemma": row.lemma,
                "gloss": row.gloss,
                "examples": row.examples,
            }))
        })
        .collect();
    let def_total = def_payload.len();

    // --- Definitions ---
    let existing_def_count: i64 = sqlx::query_scalar(
        r#"SELECT COUNT(*) FROM "DictionaryDefinition" WHERE source = 'WORDNET'"#,
    )
    .fetch_one(&pool)
    .await?;

    if existing_def_count < def_total as i64 {
        let mut completed = 0usize;
        for chunk in def_payload.chunks(DEFINITION_CHUNK_SIZE) {
            let payload = Value::Array(chunk.to_vec());
            let payload_str = serde_json::to_string(&payload)?;

            sqlx::query(
                r#"
                INSERT INTO "DictionaryDefinition"
                    ("id", "wordId", "partOfSpeech", "synsetOffset", "lemma", "gloss", "examples", "source")
                SELECT
                    gen_random_uuid()::text,
                    d."wordId",
                    d."partOfSpeech"::"DictionaryPartOfSpeech",
                    d."synsetOffset",
                    d."lemma",
                    d."gloss",
                    d."examples",
                    'WORDNET'::"DictionarySource"
                FROM jsonb_to_recordset($1::jsonb) AS d(
                    "wordId" text,
                    "partOfSpeech" text,
                    "synsetOffset" text,
                    "lemma" text,
                    "gloss" text,
                    "examples" jsonb
                )
                ON CONFLICT ("wordId", "partOfSpeech", "synsetOffset") DO NOTHING
                "#,
            )
            .bind(&payload_str)
            .execute(&pool)
            .await
            .context("failed to insert DictionaryDefinition chunk")?;

            completed += chunk.len();
            println!(
                "WordNet definitions: {}/{}",
                completed.min(def_total),
                def_total
            );
        }
    } else {
        println!("WordNet definitions already present ({existing_def_count}/{def_total}).");
    }

    pool.close().await;

    println!("Seeded {word_total} WordNet words.");
    println!("Seeded {def_total} WordNet definitions.");
    Ok(())
}
