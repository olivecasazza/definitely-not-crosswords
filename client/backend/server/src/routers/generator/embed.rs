//! ONNX topic-relevance embedding — the Rust replacement for the
//! `@xenova/transformers` MiniLM pipeline in generateCrossword.ts. Loads the
//! same quantized `all-MiniLM-L6-v2` model with `ort` and tokenizes with the
//! HF `tokenizers` crate, then mean-pools over tokens (attention-mask weighted)
//! and L2-normalizes — so a bare dot product equals cosine similarity, exactly
//! as the TS `cosineSimilarity` assumed. Candidates run through the model in
//! padded batches (`BATCH_SIZE` texts per `Session::run`), not one at a time.

use ort::session::{builder::GraphOptimizationLevel, Session};
use ort::value::Value as OrtValue;
use serde_json::{json, Value};
use std::path::Path;
use std::sync::{Mutex, OnceLock};
use tokenizers::Tokenizer;

const MODEL_PATH: &str = "data/crossword/models/all-MiniLM-L6-v2/onnx/model_quantized.onnx";
const TOKENIZER_PATH: &str = "data/crossword/models/all-MiniLM-L6-v2/tokenizer.json";
const CANDIDATE_LIMIT: usize = 4000;
// Texts per ONNX run (and the progress-report cadence). Measured on 4000
// length-sorted synthetic candidates (release build; both on a free 24-core
// box and pinned to 2 cores to mimic the pod's 1500m cpu limit): 32 is the
// only size that edges out the single-text loop at all (~1.05-1.07x), and
// 64/128/256 are net LOSSES (0.83-0.88x / 0.73-0.83x / 0.65-0.79x). The
// dynamically quantized graph re-estimates activation scales per run over the
// whole [B*T, hidden] tensor, so larger batches buy no amortization — just
// bigger working sets. Numbers from bench_embed_batched_vs_single.
const BATCH_SIZE: usize = 32;

struct EmbedModel {
    // Session::run needs &mut self; a Mutex gives it through the &'static
    // model. Any signed-in user can trigger generation (the WS handler spawns
    // a detached task per request), so concurrent jobs DO contend here and
    // serialize per Session::run. Batching coarsens that serialization from
    // one text (~3ms) to one chunk (~90ms) per acquisition; total inference
    // time is roughly unchanged (see BATCH_SIZE), so in the worst case a
    // queued job still waits out a rival's whole embedding stage.
    session: Mutex<Session>,
    tokenizer: Tokenizer,
}

// ponytail: lazy global, may load twice under a race (harmless — the loser's
// Session is dropped, only wasted load work); upgrade to get_or_try_init if
// that ever stabilizes.
static MODEL: OnceLock<EmbedModel> = OnceLock::new();

fn load_model(
    model_path: impl AsRef<Path>,
    tokenizer_path: impl AsRef<Path>,
) -> Result<EmbedModel, String> {
    let tokenizer = Tokenizer::from_file(tokenizer_path.as_ref()).map_err(|e| {
        format!(
            "failed to load tokenizer ({}): {e}",
            tokenizer_path.as_ref().display()
        )
    })?;
    let session = Session::builder()
        .map_err(|e| e.to_string())?
        .with_optimization_level(GraphOptimizationLevel::Level3)
        .map_err(|e| e.to_string())?
        .commit_from_file(model_path.as_ref())
        .map_err(|e| {
            format!(
                "failed to load model ({}): {e}",
                model_path.as_ref().display()
            )
        })?;
    Ok(EmbedModel {
        session: Mutex::new(session),
        tokenizer,
    })
}

fn model() -> Result<&'static EmbedModel, String> {
    if let Some(m) = MODEL.get() {
        return Ok(m);
    }
    let loaded = load_model(MODEL_PATH, TOKENIZER_PATH)?;
    let _ = MODEL.set(loaded);
    Ok(MODEL.get().unwrap())
}

/// Mean-pool one row of `last_hidden_state` under its attention mask, then
/// L2-normalize. `data` is the row's `[len, dim]` slice; `mask` its mask.
fn pool_normalize(data: &[f32], mask: &[i64], dim: usize) -> Vec<f32> {
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
    pooled
}

/// Embed one text to a normalized sentence vector (mean-pool + L2-normalize).
/// Used for the topic (and kept as the reference path for the equivalence
/// test); candidates go through `embed_batch`.
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
    Ok(pool_normalize(data, &mask, dim))
}

/// Embed a batch of texts in one `Session::run`: pad every row to the longest
/// encoding in the batch (BERT's `[PAD]` is token id 0, mask 0, type 0), run
/// `[B, max_len]`, then mean-pool each row under ITS OWN attention mask — so
/// padding contributes nothing to pooling and each row's semantics match
/// `embed_one`.
///
/// Numerical caveat (measured, see tests): this model is DYNAMICALLY quantized
/// — activation scales are computed per-tensor over the whole `[B*T, hidden]`
/// activation at run time, so batchmates change every row's rounding. A batch
/// of identical rows reproduces `embed_one` bit-exactly, but mixed batches
/// drift by up to ~2e-2 in cosine even with zero padding. That drift is the
/// same order as the model's own quantization error vs fp32 — batched output
/// is a different-but-equally-valid rounding of the same model, and it stays
/// deterministic for a given candidate list (chunking follows the stable
/// quality sort). Exact bit-equality with the single-text path is impossible
/// while the model is dynamically quantized.
fn embed_batch(m: &EmbedModel, texts: &[&str]) -> Result<Vec<Vec<f32>>, String> {
    if texts.is_empty() {
        return Ok(Vec::new());
    }
    let encs = m
        .tokenizer
        .encode_batch(texts.to_vec(), true)
        .map_err(|e| e.to_string())?;
    let b = encs.len();
    let max_len = encs.iter().map(|e| e.get_ids().len()).max().unwrap_or(1);

    // Zero-initialized = already padded ([PAD]=0 / mask 0 / type 0).
    let mut ids = vec![0i64; b * max_len];
    let mut mask = vec![0i64; b * max_len];
    let mut types = vec![0i64; b * max_len];
    for (r, enc) in encs.iter().enumerate() {
        let row = r * max_len;
        for (t, &v) in enc.get_ids().iter().enumerate() {
            ids[row + t] = v as i64;
        }
        for (t, &v) in enc.get_attention_mask().iter().enumerate() {
            mask[row + t] = v as i64;
        }
        for (t, &v) in enc.get_type_ids().iter().enumerate() {
            types[row + t] = v as i64;
        }
    }
    let shape = [b as i64, max_len as i64];

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

    // `outputs` borrows the session, so the lock is held through the pooling
    // below — fine, pooling is microseconds next to the model run.
    // last_hidden_state: [B, max_len, dim]
    let (_shape, data) = outputs[0]
        .try_extract_tensor::<f32>()
        .map_err(|e| e.to_string())?;
    let dim = data.len() / (b * max_len);

    let mut out = Vec::with_capacity(b);
    for r in 0..b {
        let row_data = &data[r * max_len * dim..(r + 1) * max_len * dim];
        let row_mask = &mask[r * max_len..(r + 1) * max_len];
        out.push(pool_normalize(row_data, row_mask, dim));
    }
    Ok(out)
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
/// candidates in batches of `BATCH_SIZE`, and return
/// `word -> cosine(topic, candidate)`. Emits the same `embedding-model` /
/// `embedding` stage + progress events as the TS pipeline.
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
    // Length-sort before chunking (order is free — scores land in a HashMap,
    // and the sort is stable so it's deterministic). Every row in a batch is
    // padded to the longest row and attention is O(len^2), so mixed-length
    // chunks burn compute on padding: measured, naive quality-order chunking
    // was 0.62-0.97x vs the single-text loop; length-sorting is what gets
    // batching back to >=1x. Less padding also means less quantization drift
    // (see embed_batch).
    candidates.sort_by_key(|c| c.embed_text.len());
    let total_batches = candidates.len().div_ceil(BATCH_SIZE);

    emit(json!({
        "type": "stage", "stage": "embedding",
        "message": format!("Scoring {} candidates for topic relevance", candidates.len()),
    }));

    let mut scores = std::collections::HashMap::new();
    for (bi, chunk) in candidates.chunks(BATCH_SIZE).enumerate() {
        let texts: Vec<&str> = chunk.iter().map(|c| c.embed_text.as_str()).collect();
        let embs = embed_batch(m, &texts)?;
        for (cand, emb) in chunk.iter().zip(embs.iter()) {
            scores.insert(cand.word.clone(), cosine(&topic_emb, emb) as f64);
        }
        emit(json!({
            "type": "progress", "stage": "embedding",
            "current": bi + 1, "total": total_batches,
            "message": "Embedding candidate words",
        }));
    }
    Ok(scores)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    // `cargo test` runs with cwd = this crate (client/backend/server); the
    // model assets live at the REPO root under data/ (gitignored, hash-fetched
    // by nix — see data/crossword/manifest.json). Resolve them from
    // CARGO_MANIFEST_DIR so the tests don't depend on cwd.
    fn repo_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..")
    }

    // Load a private model instance (not the global OnceLock) so tests don't
    // depend on process cwd. Returns None — and the caller skips — when data/
    // is absent (e.g. a bare CI checkout); run locally from a checkout where
    // `data/crossword/models` is populated (scripts/prepare or the nix asset
    // derivation).
    fn test_model() -> Option<EmbedModel> {
        let root = repo_root();
        let model_path = root.join(MODEL_PATH);
        let tok_path = root.join(TOKENIZER_PATH);
        if !model_path.exists() || !tok_path.exists() {
            eprintln!(
                "SKIP: embedding model assets not found under {} — populate data/crossword to run",
                root.display()
            );
            return None;
        }
        Some(load_model(&model_path, &tok_path).expect("assets present but failed to load"))
    }

    // A batch of identical rows hits the exact same tensor min/max as a single
    // run, so the dynamic-quant scales match and the result must be BIT-exact.
    // This is the strong assertion: it proves the padding/pooling/mask plumbing
    // itself introduces zero error.
    #[test]
    fn batched_identical_rows_are_bit_exact() {
        let Some(m) = test_model() else { return };
        for text in ["sun", "orbit: the curved path of a celestial object"] {
            let single = embed_one(&m, text).unwrap();
            let batched = embed_batch(&m, &[text, text, text]).unwrap();
            for (r, row) in batched.iter().enumerate() {
                assert_eq!(
                    &single, row,
                    "row {r} of an identical-rows batch differs from embed_one({text:?})"
                );
            }
        }
    }

    #[test]
    fn batched_embedding_matches_single() {
        let Some(m) = test_model() else { return };
        // Deliberately varied lengths so batching forces real padding.
        let texts: Vec<&str> = vec![
            "sun",
            "planet: a large body orbiting a star; a wanderer of the night sky",
            "galaxy: a system of millions or billions of stars, together with gas and dust, held together by gravitational attraction",
            "cheese: a food made from the pressed curds of milk",
            "orbit: the curved path of a celestial object around a star, planet, or moon; one complete circuit",
            "xylophone",
        ];
        let topic = "the solar system and space exploration";
        let topic_emb = embed_one(&m, topic).unwrap();

        let batched = embed_batch(&m, &texts).unwrap();
        assert_eq!(batched.len(), texts.len());

        // Epsilon: NOT the usual fp-reassociation 1e-4-ish. The model is
        // DYNAMICALLY quantized: activation quantization scales are computed
        // per-tensor at run time, so batchmates (content AND padding) change
        // every row's rounding — measured drift is up to ~2e-2 in self-cosine
        // for extreme short texts ("cat" in a mixed batch: 0.982), zero for
        // identical-row batches (see batched_identical_rows_are_bit_exact,
        // which pins the plumbing down bit-exactly). 3e-2 bounds the measured
        // quantization drift with a little margin while still catching real
        // bugs: a single mis-masked pad token or an off-by-one row slice
        // shifts cosine by ~1e-1, well above it.
        const EPS: f32 = 3e-2;

        for (i, text) in texts.iter().enumerate() {
            let single = embed_one(&m, text).unwrap();
            let self_sim = cosine(&single, &batched[i]);
            assert!(
                (1.0 - self_sim).abs() < EPS,
                "row {i} ({text:?}): batched vs single self-cosine {self_sim} drifted > {EPS}"
            );
            let s_single = cosine(&topic_emb, &single);
            let s_batched = cosine(&topic_emb, &batched[i]);
            eprintln!("row {i}: topic score single {s_single:.5} batched {s_batched:.5}");
            assert!(
                (s_single - s_batched).abs() < EPS,
                "row {i} ({text:?}): topic score {s_batched} vs {s_single} drifted > {EPS}"
            );
        }
    }

    // Honest before/after timing on ~4000 synthetic candidates. Run locally:
    //   cargo test -p crossword-server --release -- --ignored bench_embed --nocapture
    #[test]
    #[ignore = "benchmark; minutes of CPU"]
    fn bench_embed_batched_vs_single() {
        let Some(m) = test_model() else { return };
        let texts: Vec<String> = (0..4000)
            .map(|i| {
                // Length spread comparable to real "word: gloss; gloss" texts.
                let extra = " with several additional glossary words".repeat(i % 4);
                format!("word{i}: a short dictionary gloss about subject number {i}{extra}")
            })
            .collect();
        let mut refs: Vec<&str> = texts.iter().map(|s| s.as_str()).collect();
        let topic_emb = embed_one(&m, "an encyclopedia of general knowledge").unwrap();

        // Mirror score_candidates: length-sort before chunking. Without this,
        // mixed-length chunks pad every row to the chunk max and batching is
        // a WASH (measured 0.97x at batch 32, 0.62x at 64 on quality-order
        // chunking of this same set). Sorted up front so single_embs and
        // batched_embs stay row-aligned for the fidelity comparison.
        refs.sort_by_key(|t| t.len());

        let t0 = std::time::Instant::now();
        let single_embs: Vec<Vec<f32>> = refs.iter().map(|t| embed_one(&m, t).unwrap()).collect();
        let single = t0.elapsed();

        // Batch-size sweep on the batched path (cheap relative to the single
        // baseline); BATCH_SIZE should sit on the flat part of this curve.
        for bs in [32, 64, 128, 256] {
            let t1 = std::time::Instant::now();
            let mut batched_embs: Vec<Vec<f32>> = Vec::with_capacity(refs.len());
            for chunk in refs.chunks(bs) {
                batched_embs.extend(embed_batch(&m, chunk).unwrap());
            }
            let batched = t1.elapsed();

            // Score fidelity vs the single path at this batch size.
            let (mut max_d, mut sum_d) = (0f32, 0f32);
            for (s, b) in single_embs.iter().zip(&batched_embs) {
                let d = (cosine(&topic_emb, s) - cosine(&topic_emb, b)).abs();
                max_d = max_d.max(d);
                sum_d += d;
            }
            eprintln!(
                "batch {bs:>3}: {batched:>8.2?} | speedup {:>5.2}x | topic-score delta max {max_d:.4} mean {:.5}",
                single.as_secs_f64() / batched.as_secs_f64(),
                sum_d / refs.len() as f32,
            );
        }
        eprintln!("single-text baseline for 4000 candidates: {single:.2?}");
    }
}
