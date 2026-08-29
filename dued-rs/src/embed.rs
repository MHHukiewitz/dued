use std::path::Path;

use rusqlite::{params, Connection};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

pub const DEFAULT_MODEL: &str = "jinaai/jina-embeddings-v2-base-code";
#[cfg(feature = "jina")]
const MAX_SEQ: usize = 1024;
const BATCH: usize = 8;

pub fn use_stub(model_name: &str) -> bool {
    model_name == "stub" || std::env::var("DUED_STUB_EMBED").ok().as_deref() == Some("1")
}

fn stub_vector(text: &str) -> Vec<f32> {
    let digest = Sha256::digest(text.as_bytes());
    let mut raw = Vec::with_capacity(64);
    while raw.len() < 64 {
        raw.extend_from_slice(&digest);
    }
    raw.truncate(64);
    let mut v: Vec<f32> = raw.into_iter().map(|b| b as f32).collect();
    let norm = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm != 0.0 {
        for x in &mut v {
            *x /= norm;
        }
    }
    v
}

fn to_bytes(v: &[f32]) -> Vec<u8> {
    let mut out = Vec::with_capacity(v.len() * 4);
    for x in v {
        out.extend_from_slice(&x.to_le_bytes());
    }
    out
}

fn from_bytes(blob: &[u8]) -> Vec<f32> {
    blob.chunks(4)
        .filter_map(|c| c.try_into().ok().map(f32::from_le_bytes))
        .collect()
}

fn cosine(a: &[f32], b: &[f32]) -> f64 {
    if a.is_empty() || b.is_empty() || a.len() != b.len() {
        return 0.0;
    }
    let dot: f32 = a.iter().zip(b).map(|(x, y)| x * y).sum();
    let na = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let nb = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    if na == 0.0 || nb == 0.0 {
        0.0
    } else {
        (dot / (na * nb)) as f64
    }
}

enum EmbedBackend {
    Stub,
    #[cfg(feature = "jina")]
    Jina(JinaEmbedder),
}

impl EmbedBackend {
    fn load(model_name: &str) -> Self {
        if use_stub(model_name) {
            return Self::Stub;
        }
        #[cfg(feature = "jina")]
        {
            return Self::Jina(JinaEmbedder::load(model_name));
        }
        #[cfg(not(feature = "jina"))]
        panic!(
            "real Jina embeddings need dued-rs built with --features jina; \
             use --model stub, DUED_STUB_EMBED=1, or --no-embed"
        );
    }

    fn encode(&mut self, texts: &[String]) -> Vec<Vec<f32>> {
        match self {
            Self::Stub => texts.iter().map(|t| stub_vector(t)).collect(),
            #[cfg(feature = "jina")]
            Self::Jina(model) => model.encode(texts),
        }
    }
}

pub fn embed_symbols(conn: &Connection, model_name: &str, only_missing: bool) {
    let sql = if only_missing {
        "SELECT id, name, signature, docstring, body FROM symbols WHERE embed_body IS NULL"
    } else {
        "SELECT id, name, signature, docstring, body FROM symbols"
    };
    let mut stmt = conn.prepare(sql).unwrap();
    let rows: Vec<(i64, String, String, String, String)> = stmt
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)))
        .unwrap()
        .flatten()
        .collect();
    let mut backend = EmbedBackend::load(model_name);
    let total = rows.len();
    let mut bar = crate::progress::Bar::new("embed", total);
    for (i, chunk) in rows.chunks(BATCH).enumerate() {
        let sigs: Vec<String> = chunk
            .iter()
            .map(|(_, name, sig, _, _)| if sig.is_empty() { name.clone() } else { sig.clone() })
            .collect();
        let docs: Vec<String> = chunk
            .iter()
            .map(|(_, name, sig, doc, _)| {
                if !doc.is_empty() {
                    doc.clone()
                } else if !sig.is_empty() {
                    sig.clone()
                } else {
                    name.clone()
                }
            })
            .collect();
        let bodies: Vec<String> = chunk
            .iter()
            .map(|(_, _, _, _, body)| body.chars().take(8000).collect())
            .collect();
        let vs = backend.encode(&sigs);
        let vd = backend.encode(&docs);
        let vb = backend.encode(&bodies);
        for ((id, _, _, _, _), (s, d, b)) in chunk.iter().zip(vs.iter().zip(vd.iter()).zip(vb.iter()).map(|((s, d), b)| (s, d, b))) {
            conn.execute(
                "UPDATE symbols SET embed_sig = ?, embed_doc = ?, embed_body = ? WHERE id = ?",
                params![to_bytes(s), to_bytes(d), to_bytes(b), *id],
            )
            .ok();
        }
        bar.set(((i + 1) * BATCH).min(total), &chunk[0].1);
    }
    bar.finish();
}

#[cfg(feature = "jina")]
struct JinaEmbedder {
    session: ort::session::Session,
    tokenizer: tokenizers::Tokenizer,
    input_names: Vec<String>,
    output_name: String,
}

#[cfg(feature = "jina")]
impl JinaEmbedder {
    fn load(model_name: &str) -> Self {
        crate::progress::note(&format!("load Jina model {model_name}"));
        crate::progress::note("if the model is not cached, this step downloads ONNX weights");
        crate::progress::note("the download can take several minutes on the first run");
        let api = hf_hub::api::sync::Api::new().expect("huggingface API");
        let repo = api.model(model_name.to_string());
        let onnx_name = std::env::var("DUED_JINA_ONNX").unwrap_or_else(|_| "onnx/model_quantized.onnx".into());
        let onnx_path = repo.get(&onnx_name).unwrap_or_else(|_| {
            panic!("failed to download {model_name} / {onnx_name}. check network access")
        });
        let tok_path = repo
            .get("tokenizer.json")
            .expect("failed to download tokenizer.json");
        let mut tokenizer = tokenizers::Tokenizer::from_file(&tok_path).expect("load tokenizer.json");
        let _ = tokenizer.with_truncation(Some(tokenizers::TruncationParams {
            max_length: MAX_SEQ,
            ..Default::default()
        }));
        tokenizer.with_padding(Some(tokenizers::PaddingParams {
            strategy: tokenizers::PaddingStrategy::BatchLongest,
            ..Default::default()
        }));
        let session = ort::session::Session::builder()
            .expect("onnx session builder")
            .commit_from_file(&onnx_path)
            .unwrap_or_else(|_| panic!("failed to open Jina ONNX {}", onnx_path.display()));
        let input_names: Vec<String> = session.inputs().iter().map(|i| i.name().to_string()).collect();
        let output_name = session
            .outputs()
            .iter()
            .map(|o| o.name().to_string())
            .find(|n| n == "last_hidden_state" || n == "sentence_embedding")
            .or_else(|| session.outputs().first().map(|o| o.name().to_string()))
            .expect("Jina ONNX has no outputs");
        crate::progress::note(&format!("Jina ONNX ready ({onnx_name})"));
        Self {
            session,
            tokenizer,
            input_names,
            output_name,
        }
    }

    fn encode(&mut self, texts: &[String]) -> Vec<Vec<f32>> {
        if texts.is_empty() {
            return Vec::new();
        }
        let encodings = self
            .tokenizer
            .encode_batch(texts.to_vec(), true)
            .expect("tokenize");
        let batch = encodings.len();
        let seq = encodings.iter().map(|e| e.get_ids().len()).max().unwrap_or(0);
        let mut ids = vec![0i64; batch * seq];
        let mut mask = vec![0i64; batch * seq];
        let types = vec![0i64; batch * seq];
        for (i, enc) in encodings.iter().enumerate() {
            let row = i * seq;
            for (j, (id, m)) in enc.get_ids().iter().zip(enc.get_attention_mask()).enumerate() {
                ids[row + j] = i64::from(*id);
                mask[row + j] = i64::from(*m);
            }
        }
        let ids_t = ort::value::Tensor::from_array(([batch, seq], ids)).expect("input_ids tensor");
        let mask_t = ort::value::Tensor::from_array(([batch, seq], mask.clone())).expect("attention_mask tensor");
        let types_t = ort::value::Tensor::from_array(([batch, seq], types)).expect("token_type_ids tensor");
        let mut feeds: Vec<(String, ort::value::Value)> = Vec::new();
        if self.input_names.iter().any(|n| n == "input_ids") {
            feeds.push(("input_ids".into(), ids_t.into()));
        }
        if self.input_names.iter().any(|n| n == "attention_mask") {
            feeds.push(("attention_mask".into(), mask_t.into()));
        }
        if self.input_names.iter().any(|n| n == "token_type_ids") {
            feeds.push(("token_type_ids".into(), types_t.into()));
        }
        let outputs = self.session.run(feeds).expect("Jina ONNX inference");
        let hidden = outputs
            .get(self.output_name.as_str())
            .unwrap_or_else(|| &outputs[0]);
        let (shape, data) = hidden.try_extract_tensor::<f32>().expect("extract f32 embeddings");
        if self.output_name == "sentence_embedding" || shape.len() == 2 {
            let dim = shape[1] as usize;
            return (0..batch).map(|i| normalize(&data[i * dim..(i + 1) * dim])).collect();
        }
        let seq_out = shape[1] as usize;
        let dim = shape[2] as usize;
        (0..batch)
            .map(|i| {
                mean_pool(&data[i * seq_out * dim..(i + 1) * seq_out * dim], &mask[i * seq..], seq_out, dim)
            })
            .collect()
    }
}

#[cfg(feature = "jina")]
fn mean_pool(tokens: &[f32], mask: &[i64], seq: usize, dim: usize) -> Vec<f32> {
    let mut out = vec![0.0f32; dim];
    let mut count = 0.0f32;
    for t in 0..seq {
        if mask.get(t).copied().unwrap_or(0) == 0 {
            continue;
        }
        count += 1.0;
        let off = t * dim;
        for d in 0..dim {
            out[d] += tokens[off + d];
        }
    }
    if count > 0.0 {
        for x in &mut out {
            *x /= count;
        }
    }
    normalize(&out)
}

#[cfg(feature = "jina")]
fn normalize(v: &[f32]) -> Vec<f32> {
    let n = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    if n == 0.0 {
        v.to_vec()
    } else {
        v.iter().map(|x| x / n).collect()
    }
}

pub fn mismatch_flags(conn: &Connection) -> Vec<Value> {
    let mut stmt = conn
        .prepare(
            r#"
        SELECT s.id, s.name, s.docstring, s.embed_sig, s.embed_doc, s.embed_body, f.relpath, s.start_line
        FROM symbols s JOIN files f ON f.id = s.file_id
        WHERE s.embed_body IS NOT NULL
        "#,
        )
        .unwrap();
    let rows: Vec<(i64, String, String, Vec<u8>, Vec<u8>, Vec<u8>, String, i64)> = stmt
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?, r.get(7)?)))
        .unwrap()
        .flatten()
        .collect();
    if rows.is_empty() {
        return Vec::new();
    }
    let mut doc_body = Vec::new();
    let mut sig_body = Vec::new();
    for row in &rows {
        let body = from_bytes(&row.5);
        doc_body.push(cosine(&from_bytes(&row.4), &body));
        sig_body.push(cosine(&from_bytes(&row.3), &body));
    }
    let mut db_sorted = doc_body.clone();
    db_sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let mut sb_sorted = sig_body.clone();
    sb_sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let doc_cut = quantile(&db_sorted, 0.15);
    let sig_cut = quantile(&sb_sorted, 0.15);
    let mut flags = Vec::new();
    for (i, row) in rows.iter().enumerate() {
        if !row.2.is_empty() && doc_body[i] <= doc_cut {
            flags.push(json!({
                "kind": "doc_body_mismatch",
                "name": row.1,
                "relpath": row.6,
                "start_line": row.7,
                "score": doc_body[i],
                "detail": format!("doc↔body cosine {:.3} in bottom quantile", doc_body[i]),
            }));
            conn.execute(
                "INSERT INTO name_flags(symbol_id, kind, detail, score) VALUES (?,?,?,?)",
                params![row.0, "doc_body_mismatch", format!("doc↔body cosine {:.3}", doc_body[i]), doc_body[i]],
            )
            .ok();
        }
        if sig_body[i] <= sig_cut {
            flags.push(json!({
                "kind": "signature_body_mismatch",
                "name": row.1,
                "relpath": row.6,
                "start_line": row.7,
                "score": sig_body[i],
                "detail": format!("signature↔body cosine {:.3} in bottom quantile", sig_body[i]),
            }));
        }
    }
    flags
}

fn quantile(sorted: &[f64], q: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    let idx = ((sorted.len() as f64 - 1.0) * q).round() as usize;
    sorted[idx.min(sorted.len() - 1)]
}

pub fn similar_lookup_error(conn: &Connection, query: &str) -> Option<String> {
    let mut stmt = conn
        .prepare(
            r#"
        SELECT s.name, f.relpath, s.embed_body
        FROM symbols s JOIN files f ON f.id = s.file_id
        "#,
        )
        .unwrap();
    let rows: Vec<(String, String, Option<Vec<u8>>)> = stmt
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .unwrap()
        .flatten()
        .collect();
    let target = if let Some((path, name)) = query.rsplit_once("::") {
        rows.iter().find(|r| r.1 == path && r.0 == name)
    } else {
        rows.iter().find(|r| r.0 == query)
    };
    match target {
        None => Some("symbol not found".to_string()),
        Some((_, _, None)) => Some("symbol has no embedding; scan without --no-embed".to_string()),
        Some((_, _, Some(_))) => None,
    }
}

pub fn similar_to(conn: &Connection, query: &str) -> Vec<Value> {
    let mut stmt = conn
        .prepare(
            r#"
        SELECT s.id, s.name, s.signature, f.relpath, s.embed_body
        FROM symbols s JOIN files f ON f.id = s.file_id
        WHERE s.embed_body IS NOT NULL
        "#,
        )
        .unwrap();
    let rows: Vec<(i64, String, String, String, Vec<u8>)> = stmt
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)))
        .unwrap()
        .flatten()
        .collect();
    let target = if let Some((path, name)) = query.rsplit_once("::") {
        rows.iter().find(|r| r.3 == path && r.1 == name)
    } else {
        rows.iter().find(|r| r.1 == query)
    };
    let Some(target) = target else {
        return Vec::new();
    };
    let tv = from_bytes(&target.4);
    let mut scored: Vec<(f64, &(i64, String, String, String, Vec<u8>))> = rows
        .iter()
        .filter(|r| r.0 != target.0)
        .map(|r| (cosine(&tv, &from_bytes(&r.4)), r))
        .collect();
    scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap());
    scored
        .into_iter()
        .take(10)
        .map(|(score, row)| json!({"name": row.1, "relpath": row.3, "signature": row.2, "score": score}))
        .collect()
}

pub fn export_label_csv(conn: &Connection, dest: &Path) -> usize {
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent).ok();
    }
    let mut stmt = conn
        .prepare(
            r#"
        SELECT s.name, f.relpath, n.kind, n.score
        FROM name_flags n JOIN symbols s ON s.id = n.symbol_id JOIN files f ON f.id = s.file_id
        WHERE n.kind IN ('doc_body_mismatch', 'signature_body_mismatch', 'homonym')
        "#,
        )
        .unwrap();
    let rows: Vec<(String, String, String, f64)> = stmt
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
        .unwrap()
        .flatten()
        .collect();
    let mut lines = vec!["name,relpath,kind,score,human_label".to_string()];
    for row in &rows {
        lines.push(format!("{},{},{},{:.4},", row.0, row.1, row.2, row.3));
    }
    std::fs::write(dest, lines.join("\n") + "\n").ok();
    rows.len()
}
