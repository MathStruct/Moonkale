//! Hybrid search over the indexed text: BM25 over chunks, optionally fused
//! with cosine similarity of chunk embeddings (reciprocal-rank fusion).
//! Everything is in memory and rebuilt with the index; embeddings are
//! filled in by [`crate::IndexSource`] when an embedder is configured.

use moonkale_core::{Node, NodeId};
use std::collections::HashMap;

/// ~40 lines per chunk, split on blank lines when possible.
const CHUNK_LINES: usize = 40;
const MIN_CHUNK_LINES: usize = 8;

#[derive(Clone, Debug)]
pub struct Chunk {
    pub file: NodeId,
    pub path: String,
    /// 0-based inclusive line range.
    pub start_line: u32,
    pub end_line: u32,
    pub text: String,
    tokens: Vec<String>,
    pub embedding: Option<Vec<f32>>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Hit {
    pub file: NodeId,
    pub path: String,
    pub line: u32,
    pub score: f32,
    pub snippet: String,
}

#[derive(Default)]
pub struct SearchIndex {
    chunks: Vec<Chunk>,
    /// Document frequency per token.
    df: HashMap<String, u32>,
    total_tokens: usize,
}

impl SearchIndex {
    pub fn chunk_count(&self) -> usize {
        self.chunks.len()
    }

    pub fn embedded_count(&self) -> usize {
        self.chunks.iter().filter(|c| c.embedding.is_some()).count()
    }

    /// Replace `file`'s chunks with a fresh split of `text`.
    pub fn set_file(&mut self, file: &Node, text: &str) {
        self.remove_file(file.id);
        for (start, end, body) in split(text) {
            let tokens = tokenize(body);
            for t in tokens.iter().collect::<std::collections::HashSet<_>>() {
                *self.df.entry(t.clone()).or_default() += 1;
            }
            self.total_tokens += tokens.len();
            self.chunks.push(Chunk {
                file: file.id,
                path: file.native_key.clone(),
                start_line: start as u32,
                end_line: end as u32,
                text: body.to_string(),
                tokens,
                embedding: None,
            });
        }
    }

    pub fn remove_file(&mut self, file: NodeId) {
        let (gone, keep): (Vec<Chunk>, Vec<Chunk>) = std::mem::take(&mut self.chunks)
            .into_iter()
            .partition(|c| c.file == file);
        self.chunks = keep;
        for c in gone {
            self.total_tokens -= c.tokens.len();
            for t in c.tokens.iter().collect::<std::collections::HashSet<_>>() {
                if let Some(n) = self.df.get_mut(t) {
                    *n = n.saturating_sub(1);
                }
            }
        }
    }

    /// Chunks without an embedding yet: `(index, text)`.
    pub fn pending_embeddings(&self, max: usize) -> Vec<(usize, String)> {
        self.chunks
            .iter()
            .enumerate()
            .filter(|(_, c)| c.embedding.is_none())
            .take(max)
            .map(|(i, c)| (i, format!("{}\n{}", c.path, c.text)))
            .collect()
    }

    pub fn set_embedding(&mut self, index: usize, vector: Vec<f32>) {
        if let Some(c) = self.chunks.get_mut(index) {
            c.embedding = Some(vector);
        }
    }

    /// BM25 ranking; `query_vector` adds a cosine ranking fused by RRF.
    pub fn search(&self, query: &str, query_vector: Option<&[f32]>, limit: usize) -> Vec<Hit> {
        let q = tokenize(query);
        if q.is_empty() && query_vector.is_none() {
            return Vec::new();
        }
        let n = self.chunks.len().max(1) as f32;
        let avg = self.total_tokens as f32 / n;
        let (k1, b) = (1.2f32, 0.75f32);
        let mut bm25: Vec<(usize, f32)> = self
            .chunks
            .iter()
            .enumerate()
            .filter_map(|(i, c)| {
                let len = c.tokens.len() as f32;
                let mut score = 0.0;
                for term in &q {
                    let tf = c.tokens.iter().filter(|t| *t == term).count() as f32;
                    if tf == 0.0 {
                        continue;
                    }
                    let df = *self.df.get(term).unwrap_or(&0) as f32;
                    let idf = ((n - df + 0.5) / (df + 0.5) + 1.0).ln();
                    score +=
                        idf * (tf * (k1 + 1.0)) / (tf + k1 * (1.0 - b + b * len / avg.max(1.0)));
                }
                (score > 0.0).then_some((i, score))
            })
            .collect();
        bm25.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        let mut fused: HashMap<usize, f32> = HashMap::new();
        let k = 60.0f32;
        for (rank, (i, _)) in bm25.iter().enumerate() {
            *fused.entry(*i).or_default() += 1.0 / (k + rank as f32 + 1.0);
        }
        if let Some(qv) = query_vector {
            let mut cos: Vec<(usize, f32)> = self
                .chunks
                .iter()
                .enumerate()
                .filter_map(|(i, c)| c.embedding.as_ref().map(|e| (i, cosine(qv, e))))
                .filter(|(_, s)| *s > 0.0)
                .collect();
            cos.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
            for (rank, (i, _)) in cos.iter().take(limit * 4).enumerate() {
                *fused.entry(*i).or_default() += 1.0 / (k + rank as f32 + 1.0);
            }
        }
        let mut ranked: Vec<(usize, f32)> = fused.into_iter().collect();
        ranked.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        ranked
            .into_iter()
            .take(limit)
            .map(|(i, score)| {
                let c = &self.chunks[i];
                let (line, snippet) = best_line(c, &q);
                Hit {
                    file: c.file,
                    path: c.path.clone(),
                    line,
                    score,
                    snippet,
                }
            })
            .collect()
    }
}

/// The chunk line that matches most query terms (else the first), 1-based.
fn best_line(c: &Chunk, q: &[String]) -> (u32, String) {
    let mut best = (0usize, 0usize);
    for (i, line) in c.text.lines().enumerate() {
        let toks = tokenize(line);
        let hits = q.iter().filter(|t| toks.contains(t)).count();
        if hits > best.1 {
            best = (i, hits);
        }
    }
    let line = c.text.lines().nth(best.0).unwrap_or("").trim();
    let snippet: String = line.chars().take(160).collect();
    (c.start_line + best.0 as u32 + 1, snippet)
}

fn cosine(a: &[f32], b: &[f32]) -> f32 {
    let dot: f32 = a.iter().zip(b).map(|(x, y)| x * y).sum();
    let na: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let nb: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    if na == 0.0 || nb == 0.0 {
        0.0
    } else {
        dot / (na * nb)
    }
}

/// Lowercased alphanumeric words, 2+ chars; `snake_case` and `camelCase`
/// split into parts as well as kept whole.
pub fn tokenize(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    for word in text.split(|c: char| !c.is_alphanumeric() && c != '_') {
        if word.len() < 2 {
            continue;
        }
        let lower = word.to_lowercase();
        out.push(lower.trim_matches('_').to_string());
        let parts: Vec<String> = split_ident(word);
        if parts.len() > 1 {
            out.extend(parts);
        }
    }
    out.retain(|t| t.len() >= 2);
    out
}

fn split_ident(word: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let mut cur = String::new();
    let chars: Vec<char> = word.chars().collect();
    for (i, &c) in chars.iter().enumerate() {
        if c == '_' {
            if !cur.is_empty() {
                parts.push(std::mem::take(&mut cur));
            }
            continue;
        }
        if c.is_uppercase() && i > 0 && chars[i - 1].is_lowercase() && !cur.is_empty() {
            parts.push(std::mem::take(&mut cur));
        }
        cur.push(c.to_ascii_lowercase());
    }
    if !cur.is_empty() {
        parts.push(cur);
    }
    parts
}

/// `(start_line, end_line, text)` chunks of roughly [`CHUNK_LINES`] lines,
/// cut at blank lines when one is near the boundary.
fn split(text: &str) -> Vec<(usize, usize, &str)> {
    let lines: Vec<(usize, &str)> = text.lines().enumerate().collect();
    let offsets: Vec<usize> = {
        let mut v = Vec::with_capacity(lines.len() + 1);
        let mut o = 0;
        for l in text.lines() {
            v.push(o);
            o += l.len() + 1;
        }
        v.push(text.len());
        v
    };
    let mut out = Vec::new();
    let mut start = 0;
    while start < lines.len() {
        let mut end = (start + CHUNK_LINES).min(lines.len());
        if end < lines.len() {
            // Prefer a blank line in the last third of the window.
            if let Some(cut) = (start + MIN_CHUNK_LINES..end)
                .rev()
                .find(|&i| lines[i].1.trim().is_empty())
            {
                end = cut;
            }
        }
        if end <= start {
            end = start + 1;
        }
        let body = &text[offsets[start]..offsets[end].min(text.len())];
        if !body.trim().is_empty() {
            out.push((start, end - 1, body));
        }
        start = end;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use moonkale_core::{NodeKind, SourceId, Version};

    fn node(path: &str) -> Node {
        let sid = SourceId::new("folder:/x");
        Node {
            id: NodeId::derive(&sid, path),
            source: sid,
            kind: NodeKind::File,
            label: path.into(),
            native_key: path.into(),
            content: None,
            version: Version::default(),
        }
    }

    #[test]
    fn bm25_ranks_the_matching_file_first_with_a_line_number() {
        let mut s = SearchIndex::default();
        s.set_file(
            &node("a.md"),
            "# Alpha\n\nThis page talks about graph layouts.\n",
        );
        s.set_file(
            &node("b.rs"),
            "fn parse_stack_trace(s: &str) {}\n\nfn main() {}\n",
        );
        let hits = s.search("stack trace", None, 5);
        assert_eq!(hits[0].path, "b.rs");
        assert_eq!(hits[0].line, 1);
        assert!(hits[0].snippet.contains("parse_stack_trace"));
        assert_eq!(s.search("layouts", None, 5)[0].path, "a.md");
        assert!(s.search("zzz", None, 5).is_empty());
    }

    #[test]
    fn refresh_replaces_chunks_and_vectors_fuse() {
        let mut s = SearchIndex::default();
        s.set_file(&node("a.md"), "old words here\n");
        s.set_file(&node("a.md"), "new words here\n");
        assert_eq!(s.chunk_count(), 1);
        assert!(s.search("old", None, 5).is_empty());
        let pending = s.pending_embeddings(10);
        assert_eq!(pending.len(), 1);
        s.set_embedding(0, vec![1.0, 0.0]);
        assert_eq!(s.embedded_count(), 1);
        let hits = s.search("nothing-in-bm25", Some(&[1.0, 0.0]), 5);
        assert_eq!(hits.len(), 1, "vector-only hit");
    }

    #[test]
    fn splits_long_text_at_blank_lines() {
        let mut text = String::new();
        for i in 0..100 {
            text.push_str(&format!("line {i}\n"));
            if i % 30 == 29 {
                text.push('\n');
            }
        }
        let parts = split(&text);
        assert!(parts.len() >= 3);
        assert_eq!(parts[0].0, 0);
        assert!(parts.windows(2).all(|w| w[1].0 == w[0].1 + 1));
    }
}
