//! Full-text search over app records, with the index sealed in the vault.
//!
//! One [`Index`] per item. Its tantivy files live as sealed vault entries under
//! `search/<ws>/<item>/files/` ([`VaultDirectory`]); nothing reaches disk unsealed, and the
//! vault stays tantivy-free — to it these are opaque entries. A record is keyed `(doc, id)`.
//! What a record *is* comes from the app's `index.lua`; this crate only stores and ranks.
//! Design: docs/design/search.md.

mod dir;

use std::collections::{BTreeMap, HashMap};

use tantivy::collector::TopDocs;
use tantivy::directory::RamDirectory;
use tantivy::query::{BooleanQuery, Occur, Query, QueryParser, TermQuery};
use tantivy::schema::{
    FAST, Field, INDEXED, IndexRecordOption, STORED, STRING, Schema, TEXT, TantivyDocument, Value,
};
use tantivy::snippet::SnippetGenerator;
use tantivy::{IndexReader, IndexWriter, Order, ReloadPolicy, Term, doc};
use vault::{Vault, VaultError};

pub use dir::VaultDirectory;

#[derive(Debug, thiserror::Error)]
pub enum SearchError {
    #[error("index: {0}")]
    Tantivy(#[from] tantivy::TantivyError),
    #[error("vault: {0}")]
    Vault(#[from] VaultError),
}

/// One searchable thing, as `index.lua`'s `fields` described it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Record {
    pub doc: String,
    pub id: String,
    pub title: Option<String>,
    pub body: Option<String>,
    /// Exact-match filters: `author = "anu"` is found by `author:anu`, never by `author:an`.
    pub facets: BTreeMap<String, String>,
    pub time: Option<f64>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Hit {
    pub doc: String,
    pub id: String,
    pub score: f32,
    /// Plain text around the match — no markup, the app draws it.
    pub snippet: String,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum Rank {
    #[default]
    Relevance,
    /// Newest `time` first among the matches; records without one sort last.
    Recent,
}

#[derive(Clone, Debug, PartialEq)]
pub struct QueryOpts {
    pub limit: usize,
    pub rank: Rank,
}

impl Default for QueryOpts {
    fn default() -> Self {
        Self {
            limit: 20,
            rank: Rank::Relevance,
        }
    }
}

#[derive(Clone, Copy)]
struct Fields {
    key: Field,
    doc: Field,
    id: Field,
    title: Field,
    body: Field,
    facet: Field,
    time: Field,
}

fn schema() -> (Schema, Fields) {
    let mut b = Schema::builder();
    let f = Fields {
        key: b.add_text_field("key", STRING),
        doc: b.add_text_field("doc", STRING | STORED),
        id: b.add_text_field("id", STORED),
        title: b.add_text_field("title", TEXT | STORED),
        body: b.add_text_field("body", TEXT | STORED),
        facet: b.add_text_field("facet", STRING),
        time: b.add_f64_field("time", INDEXED | FAST),
    };
    (b.build(), f)
}

/// `(doc, id)` as one term, so a re-upsert deletes exactly its predecessor. The separator is a
/// control character no doc name or id is expected to contain.
fn key(doc: &str, id: &str) -> String {
    format!("{doc}\u{1f}{id}")
}

/// Where a doc's fingerprints live — opaque bytes here; what they mean is the caller's
/// (search.md §5), stored beside the index so they share its lifetime and its sealing.
enum PrintStore {
    Vault { vault: Vault, prefix: String },
    Memory(HashMap<String, Vec<u8>>),
}

pub struct Index {
    index: tantivy::Index,
    writer: IndexWriter,
    reader: IndexReader,
    f: Fields,
    prints: PrintStore,
    /// Staged until [`Index::commit`] lands the records they describe — a fingerprint saved
    /// ahead of its records would claim work that a crash then never finished.
    staged: BTreeMap<String, Vec<u8>>,
}

/// tantivy's floor; the arena grows in blocks, so this is a ceiling, not an allocation.
const WRITER_BUDGET: usize = 15_000_000;

impl Index {
    /// The item's index, sealed in the vault. Created empty on first open.
    pub fn open_vault(vault: &Vault, ws: &str, item: &str) -> Result<Self, SearchError> {
        let prefix = format!("search/{ws}/{item}/");
        let dir = VaultDirectory::load(vault.clone(), format!("{prefix}files/"))?;
        let prints = PrintStore::Vault {
            vault: vault.clone(),
            prefix: format!("{prefix}prints/"),
        };
        Self::build(tantivy::Index::open_or_create(dir, schema().0)?, prints)
    }

    /// For tabs that persist nothing — the app-shipped tests' tab still gets working search.
    pub fn in_memory() -> Result<Self, SearchError> {
        let index = tantivy::Index::open_or_create(RamDirectory::create(), schema().0)?;
        Self::build(index, PrintStore::Memory(HashMap::new()))
    }

    fn build(index: tantivy::Index, prints: PrintStore) -> Result<Self, SearchError> {
        let f = schema().1;
        let writer = index.writer_with_num_threads(1, WRITER_BUDGET)?;
        let reader = index
            .reader_builder()
            .reload_policy(ReloadPolicy::Manual)
            .try_into()?;
        Ok(Self {
            index,
            writer,
            reader,
            f,
            prints,
            staged: BTreeMap::new(),
        })
    }

    pub fn upsert(&mut self, r: &Record) -> Result<(), SearchError> {
        let f = self.f;
        let k = key(&r.doc, &r.id);
        self.writer.delete_term(Term::from_field_text(f.key, &k));
        let mut d = doc!(f.key => k, f.doc => r.doc.as_str(), f.id => r.id.as_str());
        if let Some(t) = &r.title {
            d.add_text(f.title, t);
        }
        if let Some(b) = &r.body {
            d.add_text(f.body, b);
        }
        for (name, value) in &r.facets {
            d.add_text(f.facet, format!("{name}={value}"));
        }
        if let Some(t) = r.time {
            d.add_f64(f.time, t);
        }
        self.writer.add_document(d)?;
        Ok(())
    }

    pub fn delete(&mut self, doc: &str, id: &str) -> Result<(), SearchError> {
        let k = key(doc, id);
        self.writer.delete_term(Term::from_field_text(self.f.key, &k));
        Ok(())
    }

    pub fn delete_doc(&mut self, doc: &str) -> Result<(), SearchError> {
        self.writer.delete_term(Term::from_field_text(self.f.doc, doc));
        Ok(())
    }

    pub fn prints(&self, doc: &str) -> Result<Option<Vec<u8>>, SearchError> {
        if let Some(p) = self.staged.get(doc) {
            return Ok(Some(p.clone()));
        }
        match &self.prints {
            PrintStore::Memory(m) => Ok(m.get(doc).cloned()),
            PrintStore::Vault { vault, prefix } => Ok(vault.get_entry(&format!("{prefix}{doc}"))?),
        }
    }

    /// Staged with the records; lands in [`Index::commit`].
    pub fn set_prints(&mut self, doc: &str, prints: Vec<u8>) {
        self.staged.insert(doc.to_string(), prints);
    }

    /// Land every upsert/delete since the last commit, then the fingerprints describing them.
    pub fn commit(&mut self) -> Result<(), SearchError> {
        self.writer.commit()?;
        self.reader.reload()?;
        for (doc, p) in std::mem::take(&mut self.staged) {
            match &mut self.prints {
                PrintStore::Memory(m) => {
                    m.insert(doc, p);
                }
                PrintStore::Vault { vault, prefix } => {
                    vault.put_entry(&format!("{prefix}{doc}"), &p)?
                }
            }
        }
        Ok(())
    }

    /// `author:anu deploy` — `name:value` words are exact facet filters, the rest is text, and
    /// every part must match. Malformed text never errors: it is read leniently.
    pub fn query(&self, q: &str, opts: &QueryOpts) -> Result<Vec<Hit>, SearchError> {
        let f = self.f;
        let mut clauses: Vec<(Occur, Box<dyn Query>)> = Vec::new();
        let mut text = Vec::new();
        for word in q.split_whitespace() {
            match word.split_once(':') {
                Some((name, value))
                    if !name.is_empty()
                        && !value.is_empty()
                        && name.chars().all(|c| c.is_alphanumeric() || c == '_') =>
                {
                    let term = Term::from_field_text(f.facet, &format!("{name}={value}"));
                    clauses.push((
                        Occur::Must,
                        Box::new(TermQuery::new(term, IndexRecordOption::Basic)),
                    ));
                }
                _ => text.push(word),
            }
        }
        let text_query = (!text.is_empty()).then(|| {
            let mut parser = QueryParser::for_index(&self.index, vec![f.title, f.body]);
            parser.set_conjunction_by_default();
            parser.set_field_boost(f.title, 2.0);
            parser.parse_query_lenient(&text.join(" ")).0
        });
        if let Some(tq) = &text_query {
            clauses.push((Occur::Must, tq.box_clone()));
        }
        if clauses.is_empty() {
            return Ok(Vec::new());
        }
        let query = BooleanQuery::new(clauses);
        let searcher = self.reader.searcher();
        let limit = opts.limit.max(1);
        let found: Vec<(f32, tantivy::DocAddress)> = match opts.rank {
            Rank::Relevance => searcher.search(&query, &TopDocs::with_limit(limit).order_by_score())?,
            Rank::Recent => searcher
                .search(
                    &query,
                    &TopDocs::with_limit(limit).order_by_fast_field::<f64>("time", Order::Desc),
                )?
                .into_iter()
                .map(|(_, addr)| (0.0, addr))
                .collect(),
        };
        let snippets = |field| match &text_query {
            Some(tq) => SnippetGenerator::create(&searcher, &**tq, field).ok(),
            None => None,
        };
        let (body_snip, title_snip) = (snippets(f.body), snippets(f.title));
        let mut hits = Vec::with_capacity(found.len());
        for (score, addr) in found {
            let d: TantivyDocument = searcher.doc(addr)?;
            let text_of = |field| {
                d.get_first(field)
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .to_string()
            };
            let snippet = [(&body_snip, f.body), (&title_snip, f.title)]
                .into_iter()
                .filter_map(|(g, field)| g.as_ref().map(|g| g.snippet(&text_of(field))))
                .map(|s| s.fragment().to_string())
                .find(|s| !s.is_empty())
                .unwrap_or_else(|| {
                    let b = text_of(f.body);
                    let s = if b.is_empty() { text_of(f.title) } else { b };
                    s.chars().take(150).collect()
                });
            hits.push(Hit {
                doc: text_of(f.doc),
                id: text_of(f.id),
                score,
                snippet,
            });
        }
        Ok(hits)
    }
}

#[cfg(test)]
mod tests;
