//! Local retrieval learning: a tiny pairwise reranker trained from annotation_log.
//!
//! The model is a 9-feature linear scorer (BM25 plus field overlap, exact phrase,
//! recency, project match and length). It starts at BM25-only weights and takes
//! one SGD step per LLM/human label, so using the product grows a user-local
//! reranker without any embedding model. The model is stored in the local
//! `learned_reranker` singleton and is never synced.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::json;

use respire::transport::local::{AnnotationCandidate, AnnotationRow, LocalStore};

use crate::annotate::{docs_from_store, tokenize, Bm25Index, RowDoc};
use crate::output::{Item as OutputItem, ResultEnvelope, Status as OutputStatus};

pub const FEATURE_COUNT: usize = 9;
pub const FEATURE_NAMES: [&str; FEATURE_COUNT] = [
    "log_bm25",
    "title_overlap",
    "head_overlap",
    "tag_overlap",
    "exact_phrase",
    "recency",
    "project_match",
    "query_len",
    "doc_len",
];

const LEARNING_RATE: f32 = 0.05;
const L2: f32 = 0.001;
/// A learned residual may reorder candidates but must not erase the BM25 base.
const RESIDUAL_LAMBDA: f32 = 0.5;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LearnedModel {
    pub weights: Vec<f32>,
    pub bias: f32,
    pub updates: u64,
}

impl Default for LearnedModel {
    fn default() -> Self {
        let mut weights = vec![0.0; FEATURE_COUNT];
        // Start at plain BM25: log_bm25 is feature 0.
        weights[0] = 1.0;
        Self { weights, bias: 0.0, updates: 0 }
    }
}

impl LearnedModel {
    fn normalized(mut self) -> Self {
        self.weights.resize(FEATURE_COUNT, 0.0);
        self
    }

    pub fn score(&self, features: &[f32]) -> f32 {
        self.bias + self.weights.iter().zip(features).map(|(weight, feature)| weight * feature).sum::<f32>()
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct TrainStep {
    pub pairs: usize,
    pub loss: f32,
    pub updates: u64,
    pub weights: Vec<f32>,
    pub bias: f32,
}

#[derive(Debug, Clone, Serialize)]
pub struct LexicalHit {
    pub id: String,
    pub title: String,
    pub snippet: String,
    pub bm25: f32,
    pub score: f32,
    pub features: Vec<f32>,
}

fn load_model(store: &LocalStore) -> Result<LearnedModel> {
    Ok(store
        .load_learned_model()?
        .map(|row| LearnedModel { weights: row.weights, bias: row.bias, updates: row.updates }.normalized())
        .unwrap_or_default())
}

fn tokens(text: &str) -> HashSet<String> {
    tokenize(text).into_iter().collect()
}

fn overlap(query: &HashSet<String>, text: &str) -> f32 {
    if query.is_empty() { return 0.0; }
    let doc = tokens(text);
    let hit = query.iter().filter(|token| doc.contains(*token)).count();
    hit as f32 / query.len() as f32
}

fn recency(created_at: &str) -> f32 {
    let Ok(created) = chrono::DateTime::parse_from_rfc3339(created_at) else { return 0.0; };
    let days = (chrono::Utc::now() - created.with_timezone(&chrono::Utc)).num_days().max(0) as f32;
    0.5_f32.powf(days / 90.0)
}

fn features(query: &str, query_tokens: &HashSet<String>, doc: &RowDoc, bm25: f32, project: &str) -> Vec<f32> {
    let query_lower = query.trim().to_lowercase();
    let title_lower = doc.title.to_lowercase();
    let head_lower = doc.head.to_lowercase();
    let exact = if query_lower.chars().count() >= 2
        && (title_lower.contains(&query_lower) || head_lower.contains(&query_lower)) { 1.0 } else { 0.0 };
    let project_match = if project.trim().is_empty() { 0.0 }
        else if doc.project == project.trim() { 1.0 } else { 0.0 };
    vec![
        (1.0 + bm25.max(0.0)).ln(),
        overlap(query_tokens, &doc.title),
        overlap(query_tokens, &doc.head),
        overlap(query_tokens, &doc.tags),
        exact,
        recency(&doc.created_at),
        project_match,
        query.chars().count() as f32 / 100.0,
        doc.head.chars().count() as f32 / 1000.0,
    ]
}

fn doc_for_candidate(candidate: &AnnotationCandidate, docs: &HashMap<String, RowDoc>) -> RowDoc {
    docs.get(&candidate.id).cloned().unwrap_or_else(|| RowDoc {
        id: candidate.id.clone(),
        title: candidate.title.clone(),
        tags: String::new(),
        head: candidate.snippet.clone(),
        project: String::new(),
        created_at: String::new(),
        kind: String::new(),
    })
}

/// One SGD step over every positive/negative pair in one annotation row.
fn train_one_with_docs(
    row: &AnnotationRow,
    docs: &HashMap<String, RowDoc>,
    model: &mut LearnedModel,
) -> Result<TrainStep> {
    let positives: Vec<&AnnotationCandidate> = row.candidates.iter().filter(|c| row.selected.contains(&c.id)).collect();
    let negatives: Vec<&AnnotationCandidate> = row.candidates.iter().filter(|c| !row.selected.contains(&c.id)).collect();
    if positives.is_empty() || negatives.is_empty() {
        return Ok(TrainStep { pairs: 0, loss: 0.0, updates: model.updates, weights: model.weights.clone(), bias: model.bias });
    }
    let query_tokens = tokens(&row.query);
    let mut gradient = vec![0.0_f32; FEATURE_COUNT];
    let mut loss = 0.0_f32;
    let mut pairs = 0_usize;
    'outer: for positive in &positives {
        let positive_doc = doc_for_candidate(positive, docs);
        let positive_features = features(&row.query, &query_tokens, &positive_doc, positive.score, &row.project);
        for negative in &negatives {
            if pairs >= 200 { break 'outer; }
            let negative_doc = doc_for_candidate(negative, docs);
            let negative_features = features(&row.query, &query_tokens, &negative_doc, negative.score, &row.project);
            let margin = model.score(&positive_features) - model.score(&negative_features);
            let probability = 1.0 / (1.0 + (-margin).exp());
            // Loss = -log(sigmoid(margin)); the step pushes positive above negative.
            loss += -probability.max(1e-6).ln();
            let delta = 1.0 - probability;
            for index in 1..FEATURE_COUNT {
                gradient[index] += delta * (positive_features[index] - negative_features[index]);
            }
            pairs += 1;
        }
    }
    if pairs == 0 {
        return Ok(TrainStep { pairs: 0, loss: 0.0, updates: model.updates, weights: model.weights.clone(), bias: model.bias });
    }
    let scale = LEARNING_RATE / pairs as f32;
    // Feature 0 is the BM25 base and stays fixed at 1.0: training may add
    // evidence but can never invert the lexical ordering on its own.
    for index in 1..FEATURE_COUNT {
        model.weights[index] = (model.weights[index] + scale * gradient[index] - L2 * model.weights[index])
            .clamp(-2.0, 2.0);
    }
    model.weights[0] = 1.0;
    model.bias = 0.0;
    model.updates += 1;
    Ok(TrainStep {
        pairs,
        loss: loss / pairs as f32,
        updates: model.updates,
        weights: model.weights.clone(),
        bias: model.bias,
    })
}

/// Train one step from a single annotation row and persist the model.
pub fn train_row(store: &LocalStore, row: &AnnotationRow) -> Result<TrainStep> {
    let docs: HashMap<String, RowDoc> = docs_from_store(store)?
        .into_iter()
        .map(|doc| (doc.id.clone(), doc))
        .collect();
    let mut model = load_model(store)?;
    let step = train_one_with_docs(row, &docs, &mut model)?;
    if step.pairs > 0 {
        store.save_learned_model(&model.weights, model.bias, model.updates)?;
    }
    Ok(step)
}

fn snippet(head: &str, title: &str) -> String {
    let text: String = head.split_whitespace().collect::<Vec<_>>().join(" ");
    let text = text.trim();
    if text.is_empty() {
        return title.chars().take(120).collect();
    }
    let mut out: String = text.chars().take(120).collect();
    if text.chars().count() > 120 { out.push('…'); }
    out
}

pub fn run_lexical(store: &LocalStore, query: &str, limit: usize, project: Option<&str>, kind: Option<&str>, titles: bool) -> Result<()> {
    let docs: Vec<RowDoc> = docs_from_store(store)?
        .into_iter()
        .filter(|doc| project.is_none_or(|p| p.is_empty() || doc.project == p))
        .filter(|doc| kind.is_none_or(|k| k.is_empty() || doc.kind.eq_ignore_ascii_case(k)))
        .collect();
    let indexed = docs.len();
    let index = Bm25Index::build(docs);
    let model = load_model(store)?;
    let query_tokens = tokens(query);
    let mut rows: Vec<(usize, f32, Vec<f32>)> = index
        .search(query, (limit.max(1) * 5).max(20))
        .into_iter()
        .filter_map(|(position, bm25)| {
            let doc = index.doc(position)?;
            let vector = features(query, &query_tokens, doc, bm25, project.unwrap_or(""));
            Some((position, bm25, vector))
        })
        .collect();
    // A model whose base weight is exactly 1 is a residual over BM25. Bound the
    // adjustment per query so learned evidence can promote a deep candidate but
    // never invert the lexical order on its own.
    let residual = (model.weights[0] - 1.0).abs() < 1e-4;
    let adjustment = |vector: &[f32]| -> f32 {
        model.bias
            + vector
                .iter()
                .skip(1)
                .zip(model.weights.iter().skip(1))
                .map(|(feature, weight)| weight * feature)
                .sum::<f32>()
    };
    let max_adjustment = if residual {
        rows.iter().map(|(_, _, vector)| adjustment(vector).abs()).fold(0.0_f32, f32::max)
    } else {
        0.0
    };
    let mut hits: Vec<LexicalHit> = rows
        .drain(..)
        .filter_map(|(position, bm25, vector)| {
            let doc = index.doc(position)?;
            let score = if residual && max_adjustment > 0.0 {
                vector[0] + RESIDUAL_LAMBDA * adjustment(&vector) / max_adjustment
            } else {
                model.score(&vector)
            };
            Some(LexicalHit {
                id: doc.id.clone(),
                title: if doc.title.trim().is_empty() { doc.head.chars().take(40).collect() } else { doc.title.clone() },
                snippet: snippet(&doc.head, &doc.title),
                bm25,
                score,
                features: vector,
            })
        })
        .collect();
    hits.sort_by(|left, right| right.score.total_cmp(&left.score).then_with(|| left.id.cmp(&right.id)));
    hits.truncate(limit.max(1));
    let count = hits.len();
    // A lexical recall is a real retrieval request: record it in query_log so
    // `query-log mark` + `learn train --from-labels` can learn from no-model use.
    let request_id = uuid::Uuid::new_v4().to_string();
    let candidate_ids: Vec<String> = hits.iter().map(|hit| hit.id.clone()).collect();
    let candidate_scores: Vec<f32> = hits.iter().map(|hit| hit.score).collect();
    if count > 0 {
        store.log_query_request(&request_id, query, project.unwrap_or(""), "", &candidate_ids, &candidate_scores)?;
    }
    let items: Vec<OutputItem> = hits.iter().map(|hit| {
        OutputItem::new(respire::service::short_id(&hit.id), OutputStatus::Ok, format!("{:.3} {}", hit.score, hit.title))
    }).collect();
    let details = if titles {
        json!(hits.iter().map(|hit| json!({"id": hit.id, "title": hit.title, "score": hit.score})).collect::<Vec<_>>())
    } else {
        json!(hits.iter().map(|hit| json!({
            "id": hit.id, "title": hit.title, "score": hit.score, "bm25": hit.bm25,
            "snippet": hit.snippet, "features": hit.features,
        })).collect::<Vec<_>>())
    };
    let mut result = ResultEnvelope::new(
        "recall",
        if count == 0 { OutputStatus::Skip } else { OutputStatus::Ok },
        json!({
            "query": query, "count": count, "limit": limit, "mode": "lexical",
            "embedding_model": null, "indexed_entries": indexed,
            "learned_updates": model.updates, "project": project,
            "query_log_id": request_id,
        }),
        items,
    );
    result.details = details;
    crate::emit_result(result)
}

pub fn status(store: &LocalStore) -> Result<()> {
    let model = load_model(store)?;
    let annotations = store.annotation_count()?;
    let mut result = ResultEnvelope::new(
        "learn",
        OutputStatus::Ok,
        json!({"updates": model.updates, "annotations": annotations, "lexical": no_embed(), "bge_available": bge_available(), "features": FEATURE_NAMES}),
        vec![OutputItem::new("updates", OutputStatus::Ok, model.updates.to_string())],
    );
    result.details = json!({
        "weights": FEATURE_NAMES.iter().zip(&model.weights)
            .map(|(name, weight)| json!({"feature": name, "weight": weight}))
            .collect::<Vec<_>>(),
        "bias": model.bias,
        "updates": model.updates,
        "annotations": annotations,
    });
    crate::emit_result(result)
}

pub fn train_from_annotations(store: &LocalStore, limit: usize, epochs: usize) -> Result<()> {
    let rows = store.annotation_training_rows(false, limit)?;
    let docs: HashMap<String, RowDoc> = docs_from_store(store)?.into_iter().map(|doc| (doc.id.clone(), doc)).collect();
    let mut model = load_model(store)?;
    let mut trained = 0_usize;
    let mut pairs = 0_usize;
    let mut loss = 0.0_f32;
    for _ in 0..epochs.max(1) {
        for row in rows.iter().rev() {
            if row.status != "ok" { continue; }
            let step = train_one_with_docs(row, &docs, &mut model)?;
            if step.pairs == 0 { continue; }
            trained += 1;
            pairs += step.pairs;
            loss += step.loss;
        }
    }
    if trained > 0 {
        store.save_learned_model(&model.weights, model.bias, model.updates)?;
    }
    let summary = json!({
        "rows": rows.len(), "trained_rows": trained, "pairs": pairs,
        "avg_loss": if trained == 0 { 0.0 } else { loss / trained as f32 },
        "updates": model.updates,
    });
    let mut result = ResultEnvelope::new("learn", OutputStatus::Ok, summary.clone(), vec![OutputItem::new("train", OutputStatus::Ok, trained.to_string())]);
    result.details = summary;
    crate::emit_result(result)
}

/// Batch-train from real adoption labels (`query_log.adopted`/`good_ids`).
///
/// BM25 stays the base score (`weights[0] = 1`); only the residual features
/// learn. Negatives are deep candidates (rank >= 20) so an adopted-but-deep
/// positive cannot teach the model to demote the lexical top-20.
pub fn train_from_labels(store: &LocalStore, limit: usize, epochs: usize) -> Result<()> {
    let docs: HashMap<String, RowDoc> = docs_from_store(store)?
        .into_iter()
        .map(|doc| (doc.id.clone(), doc))
        .collect();
    let index = Bm25Index::build(docs.values().cloned().collect());
    let rows = store.labeled_queries(limit)?;
    let mut model = load_model(store)?;
    model.weights[0] = 1.0;
    let mut trained = 0_usize;
    let mut pairs = 0_usize;
    let mut loss = 0.0_f32;
    for _ in 0..epochs.max(1) {
        for row in rows.iter().rev() {
            let candidates = index.candidates(&row.query, 100);
            let positives: Vec<&AnnotationCandidate> = candidates
                .iter()
                .filter(|candidate| row.positives.contains(&candidate.id))
                .collect();
            let negatives: Vec<&AnnotationCandidate> = candidates
                .iter()
                .enumerate()
                .filter(|(rank, candidate)| *rank >= 20 && !row.positives.contains(&candidate.id))
                .take(8)
                .map(|(_, candidate)| candidate)
                .collect();
            if positives.is_empty() || negatives.is_empty() {
                continue;
            }
            let query_tokens = tokens(&row.query);
            let mut gradient = vec![0.0_f32; FEATURE_COUNT];
            let mut local_pairs = 0_usize;
            let mut local_loss = 0.0_f32;
            'outer: for positive in &positives {
                let positive_doc = doc_for_candidate(positive, &docs);
                let positive_features = features(&row.query, &query_tokens, &positive_doc, positive.score, &row.project);
                for negative in &negatives {
                    if local_pairs >= 200 { break 'outer; }
                    let negative_doc = doc_for_candidate(negative, &docs);
                    let negative_features = features(&row.query, &query_tokens, &negative_doc, negative.score, &row.project);
                    let margin = model.score(&positive_features) - model.score(&negative_features);
                    let probability = 1.0 / (1.0 + (-margin).exp());
                    local_loss += -probability.max(1e-6).ln();
                    let delta = 1.0 - probability;
                    // Feature 0 is the BM25 base and stays fixed at 1.0.
                    for index in 1..FEATURE_COUNT {
                        gradient[index] += delta * (positive_features[index] - negative_features[index]);
                    }
                    local_pairs += 1;
                }
            }
            if local_pairs == 0 {
                continue;
            }
            let scale = LEARNING_RATE / local_pairs as f32;
            for index in 1..FEATURE_COUNT {
                model.weights[index] = (model.weights[index] + scale * gradient[index] - L2 * model.weights[index])
                    .clamp(-2.0, 2.0);
            }
            model.weights[0] = 1.0;
            model.bias = 0.0;
            model.updates += 1;
            trained += 1;
            pairs += local_pairs;
            loss += local_loss / local_pairs as f32;
        }
    }
    if trained > 0 {
        store.save_learned_model(&model.weights, model.bias, model.updates)?;
    }
    let summary = json!({
        "rows": rows.len(), "trained_queries": trained, "pairs": pairs,
        "avg_loss": if trained == 0 { 0.0 } else { loss / trained as f32 },
        "updates": model.updates,
        "weights": FEATURE_NAMES.iter().zip(&model.weights)
            .map(|(name, weight)| json!({"feature": name, "weight": weight}))
            .collect::<Vec<_>>(),
        "bias": model.bias,
    });
    let mut result = ResultEnvelope::new("learn", OutputStatus::Ok, summary.clone(), vec![OutputItem::new("train", OutputStatus::Ok, trained.to_string())]);
    result.details = summary;
    crate::emit_result(result)
}

pub fn reset(store: &LocalStore) -> Result<()> {
    store.clear_learned_model()?;
    let mut result = ResultEnvelope::new("learn", OutputStatus::Ok, json!({"reset": true}), vec![OutputItem::new("reset", OutputStatus::Ok, "model cleared")]);
    result.details = json!({"reset": true});
    crate::emit_result(result)
}

/// Detect the no-BGE mode: new users write with a zero vector and recall lexically.
pub fn lexical_marker() -> PathBuf {
    respire::service::data_dir().join("lexical.mode")
}

pub fn set_lexical(on: bool) -> Result<()> {
    let path = lexical_marker();
    if on {
        std::fs::write(&path, b"bm25\n")?;
    } else if path.exists() {
        std::fs::remove_file(&path)?;
    }
    Ok(())
}

/// A missing BGE install automatically selects BM25-only. Installing BGE later
/// switches the same profile back to hybrid without changing any flag.
pub fn bge_available() -> bool {
    respire::memory::bge::resolve_model_dir()
        .map(|dir| respire::memory::bge::model_files_present(&dir))
        .unwrap_or(false)
}

pub fn no_embed() -> bool {
    let explicit = respire::service::self_learning_enabled()
        || lexical_marker().is_file()
        || matches!(respire::env::var("RSRS_NO_EMBED"), Ok(value) if value.trim() == "1")
        || matches!(respire::env::var("RSRS_EMBED"), Ok(value) if matches!(value.trim(), "off" | "none" | "0"));
    // Capture/recall fixtures deliberately point RSRS_M3_DIR at an absent model and
    // still exercise the normal envelope. Auto-falling back to the lexical path
    // there would change the summary shape; only an explicit self switch may.
    if matches!(respire::env::var("RSRS_CORE_TEST_MODE"), Ok(value) if value.trim() == "1") {
        return explicit;
    }
    explicit || !bge_available()
}

pub fn lexical_default() -> bool {
    no_embed()
        || matches!(respire::env::var("RSRS_RETRIEVAL"), Ok(value) if matches!(value.trim(), "lexical" | "bm25" | "lexical_only"))
}


// ───────────────────────── no-BGE embedder ─────────────────────────

use respire::memory::bge::BgeEmbedder;
use respire::memory::search::Embedder;

/// A no-op provider for BM25-only users. `prepare` returns an empty artifact, so
/// the sealed row carries plaintext lexical columns but no vector/chunks. Lexical
/// recall never touches it; installing BGE later requires `reembed`.
pub struct ZeroEmbedder;

impl Embedder for ZeroEmbedder {
    fn model_name(&self) -> &str { "none" }
    fn dims(&self) -> usize { 0 }
    fn prepare(&self, _entry: &respire::memory::model::MemoryEntry) -> anyhow::Result<respire::core_sdk::Prepared> {
        Ok(respire::core_sdk::Prepared { artifact: Vec::new() })
    }
}

/// Runtime-selected embedder: BGE for hybrid users, Zero for BM25-only users.
pub enum LocalEmbedder {
    Bge(BgeEmbedder),
    Zero(ZeroEmbedder),
}

impl LocalEmbedder {
    pub fn load(model: &str) -> anyhow::Result<Self> {
        if no_embed() { Ok(Self::Zero(ZeroEmbedder)) }
        else { Ok(Self::Bge(BgeEmbedder::load_model(model)?)) }
    }

    pub fn model_name(&self) -> &str {
        match self {
            Self::Bge(embedder) => embedder.model_name(),
            Self::Zero(embedder) => embedder.model_name(),
        }
    }

    pub fn generation_key(&self) -> String {
        match self {
            Self::Bge(embedder) => embedder.generation_key().to_owned(),
            Self::Zero(_) => "none".to_owned(),
        }
    }
}

impl Embedder for LocalEmbedder {
    fn model_name(&self) -> &str {
        match self { Self::Bge(e) => e.model_name(), Self::Zero(e) => e.model_name() }
    }
    fn dims(&self) -> usize {
        match self { Self::Bge(e) => e.dims(), Self::Zero(e) => e.dims() }
    }
    fn prepare(&self, entry: &respire::memory::model::MemoryEntry) -> anyhow::Result<respire::core_sdk::Prepared> {
        match self { Self::Bge(e) => e.prepare(entry), Self::Zero(e) => e.prepare(entry) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn doc(id: &str, title: &str, head: &str) -> RowDoc {
        RowDoc {
            id: id.into(),
            title: title.into(),
            tags: String::new(),
            head: head.into(),
            project: "demo".into(),
            created_at: "2026-01-01T00:00:00Z".into(),
            kind: "context".into(),
        }
    }

    fn row() -> AnnotationRow {
        AnnotationRow {
            id: 1,
            created_at: 0,
            ts: String::new(),
            query: "部署脚本".into(),
            project: "demo".into(),
            scope: String::new(),
            topk: 2,
            candidates: vec![
                AnnotationCandidate { id: "pos".into(), title: "部署脚本位置".into(), score: 9.0, snippet: "scripts/build".into() },
                AnnotationCandidate { id: "neg".into(), title: "周报模板".into(), score: 1.0, snippet: "每周进展".into() },
            ],
            selected: vec!["pos".into()],
            confidence: 0.9,
            reason: String::new(),
            model: "test".into(),
            source: "model".into(),
            status: "ok".into(),
            raw: String::new(),
            request_id: None,
        }
    }

    #[test]
    fn default_model_is_bm25_ordered_and_features_are_finite() {
        let model = LearnedModel::default();
        let doc = doc("d", "部署脚本", "scripts/build-core-sdk.mjs");
        let tokens = tokens("部署脚本");
        let vector = features("部署脚本", &tokens, &doc, 3.0, "demo");
        assert_eq!(vector.len(), FEATURE_COUNT);
        assert!(vector.iter().all(|value| value.is_finite()));
        assert!(model.score(&vector) > 0.0);
    }

    #[test]
    fn one_step_increases_update_count_and_keeps_weights_finite() {
        let docs: HashMap<String, RowDoc> = [
            ("pos".to_owned(), doc("pos", "部署脚本位置", "scripts/build-core-sdk.mjs")),
            ("neg".to_owned(), doc("neg", "周报模板", "每周进展与风险")),
        ].into_iter().collect();
        let mut model = LearnedModel::default();
        let step = train_one_with_docs(&row(), &docs, &mut model).unwrap();
        assert_eq!(step.pairs, 1);
        assert_eq!(model.updates, 1);
        assert!(step.weights.iter().all(|weight| weight.is_finite()));
        assert!(step.bias.is_finite());
    }
}
