//! ONNX topic-relevance embedding — the Rust replacement for the
//! `@xenova/transformers` MiniLM pipeline in generateCrossword.ts. Loads the
//! same quantized `all-MiniLM-L6-v2` model with `ort` and tokenizes with the
//! HF `tokenizers` crate, then mean-pools over tokens (attention-mask weighted)
//! and L2-normalizes — so a bare dot product equals cosine similarity, exactly
//! as the TS `cosineSimilarity` assumed.

use ort::session::{builder::GraphOptimizationLevel, Session};
use ort::value::Value as OrtValue;
use serde_json::{json, Value};
use std::sync::{Mutex, OnceLock};
use tokenizers::Tokenizer;

const MODEL_PATH: &str = "data/crossword/models/all-MiniLM-L6-v2/onnx/model_quantized.onnx";
const TOKENIZER_PATH: &str = "data/crossword/models/all-MiniLM-L6-v2/tokenizer.json";
const CANDIDATE_LIMIT: usize = 4000;
const BATCH_SIZE: usize = 32; // progress-report cadence (we embed one text at a time)

struct EmbedModel {
    // Session::run needs &mut self; a Mutex gives it through the &'static model.
    // No real contention — generation is one admin job at a time.
    session: Mutex<Session>,
    tokenizer: Tokenizer,
}

// ponytail: lazy global, may load twice under a race (harmless — admin-only,
// single job at a time); upgrade to get_or_try_init if that ever stabilizes.
static MODEL: OnceLock<EmbedModel> = OnceLock::new();

fn model() -> Result<&'static EmbedModel, String> {
    if let Some(m) = MODEL.get() {
        return Ok(m);
    }
    let tokenizer = Tokenizer::from_file(TOKENIZER_PATH)
        .map_err(|e| format!("failed to load tokenizer ({TOKENIZER_PATH}): {e}"))?;
    let session = Session::builder()
        .map_err(|e| e.to_string())?
        .with_optimization_level(GraphOptimizationLevel::Level3)
        .map_err(|e| e.to_string())?
        .commit_from_file(MODEL_PATH)
        .map_err(|e| format!("failed to load model ({MODEL_PATH}): {e}"))?;
    let _ = MODEL.set(EmbedModel {
        session: Mutex::new(session),
        tokenizer,
    });
    Ok(MODEL.get().unwrap())
}

/// Embed one text to a normalized sentence vector (mean-pool + L2-normalize).
fn embed_one(m: &EmbedModel, text: &str) -> Result<Vec<f32>, String> {
    let enc = m.tokenizer.encode(text, true).map_err(|e| e.to_string())?;
    let ids: Vec<i64> = enc.get_ids().iter().map(|&x| x as i64).collect();
    let mask: Vec<i64> = enc.get_attention_mask().iter().map(|&x| x as i64).collect();
    let types: Vec<i64> = enc.get_type_ids().iter().map(|&x| x as i64).collect();
    let n = ids.len();
    let shape = [1_i64, n as i64];

    let ids_v = OrtValue::from_array((shape, ids)).map_err(|e| e.to_string())?;
    let mask_v = OrtValue::from_array((shape, mask.clone())).map_err(|e| e.to_string())?;
    let types_v = OrtValue::from_array((shape, types)).map_err(|e| e.to_string())?;

    let mut session = m.session.lock().map_err(|e| e.to_string())?;
    let outputs = session
        .run(ort::inputs![
            "input_ids" => ids_v,
            "attention_mask" => mask_v,
            "token_type_ids" => types_v,
        ])
        .map_err(|e| e.to_string())?;

    // last_hidden_state: [1, n, dim]
    let (_shape, data) = outputs[0]
        .try_extract_tensor::<f32>()
        .map_err(|e| e.to_string())?;
    let dim = data.len() / n;

    let mut pooled = vec![0f32; dim];
    let mut count = 0f32;
    for (t, &mt) in mask.iter().enumerate() {
        if mt == 0 {
            continue;
        }
        count += 1.0;
        for d in 0..dim {
            pooled[d] += data[t * dim + d];
        }
    }
    if count > 0.0 {
        for v in pooled.iter_mut() {
            *v /= count;
        }
    }
    let norm = pooled.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm > 0.0 {
        for x in pooled.iter_mut() {
            *x /= norm;
        }
    }
    Ok(pooled)
}

fn cosine(a: &[f32], b: &[f32]) -> f32 {
    a.iter().zip(b).map(|(x, y)| x * y).sum()
}

/// A candidate to score: its word (the score map key) and the text we embed
/// (`word: gloss; gloss`), with a quality score used to pick the top N.
pub struct Candidate {
    pub word: String,
    pub embed_text: String,
    pub quality: i32,
}

/// Port of `scoreCandidatesByEmbedding`: embed the topic, embed the top-quality
/// candidates, and return `word -> cosine(topic, candidate)`. Emits the same
/// `embedding-model` / `embedding` stage + progress events as the TS pipeline.
pub fn score_candidates(
    topic: &str,
    mut candidates: Vec<Candidate>,
    emit: &mut dyn FnMut(Value),
) -> Result<std::collections::HashMap<String, f64>, String> {
    emit(
        json!({ "type": "stage", "stage": "embedding-model", "message": "Loading embedding model" }),
    );
    let m = model()?;

    let topic_emb = embed_one(m, topic)?;

    candidates.sort_by_key(|c| std::cmp::Reverse(c.quality));
    candidates.truncate(CANDIDATE_LIMIT);
    let total_batches = candidates.len().div_ceil(BATCH_SIZE);

    emit(json!({
        "type": "stage", "stage": "embedding",
        "message": format!("Scoring {} candidates for topic relevance", candidates.len()),
    }));

    let mut scores = std::collections::HashMap::new();
    for (i, cand) in candidates.iter().enumerate() {
        let emb = embed_one(m, &cand.embed_text)?;
        scores.insert(cand.word.clone(), cosine(&topic_emb, &emb) as f64);
        if (i + 1) % BATCH_SIZE == 0 || i + 1 == candidates.len() {
            emit(json!({
                "type": "progress", "stage": "embedding",
                "current": (i / BATCH_SIZE) + 1, "total": total_batches,
                "message": "Embedding candidate words",
            }));
        }
    }
    Ok(scores)
}
