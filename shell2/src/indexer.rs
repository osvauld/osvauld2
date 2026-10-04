//! An open item's search index, kept current from the flush seam (docs/design/search.md §5).
//!
//! `app_host` knows `index.lua` and plans what changed; the `search` crate stores and ranks; this
//! is the join. Everything runs on the UI thread, after `flush` — the one place every writer
//! (the app, the bridge, a peer) passes through on its way to the vault.

use std::cell::RefCell;
use std::rc::Rc;

use app_host::index::IndexSpec;
use app_host::{LuaApp, SearchFn, SearchHit};
use loro::LoroDoc;
use search::{Index, QueryOpts, Rank, Record};
use vault::Vault;

pub type Shared = Rc<RefCell<ItemIndex>>;

pub struct ItemIndex {
    index: Index,
    /// Where docs the app has not opened are read from at catch-up. `None` for a test tab.
    home: Option<(Vault, String, String)>,
    /// The `index.lua` text the current spec was loaded from — compared, not hashed, on every
    /// flush: a few hundred bytes against a reload of the whole index.
    spec_src: Option<String>,
    spec: Option<IndexSpec>,
    /// `fields` calls since this index was opened: what "a restart re-indexes nothing" is
    /// measured by, over the bridge.
    pub fields_runs: usize,
    /// Bumped on every commit; a query memo keyed on it is never stale.
    generation: u64,
    memo: RefCell<Option<(String, usize, u64, Vec<SearchHit>)>>,
    /// Staged writes since the last commit.
    dirty: bool,
}

impl ItemIndex {
    pub fn open(vault: &Vault, ws: &str, item: &str) -> Result<Self, String> {
        let index = Index::open_vault(vault, ws, item).map_err(|e| e.to_string())?;
        Ok(Self::with(index, Some((vault.clone(), ws.into(), item.into()))))
    }

    pub fn in_memory() -> Result<Self, String> {
        Ok(Self::with(Index::in_memory().map_err(|e| e.to_string())?, None))
    }

    fn with(index: Index, home: Option<(Vault, String, String)>) -> Self {
        Self {
            index,
            home,
            spec_src: None,
            spec: None,
            fields_runs: 0,
            generation: 0,
            memo: RefCell::new(None),
            dirty: false,
        }
    }

    /// Cheap to call from `view`: the same query against the same commit is answered from memo.
    pub fn query(&self, q: &str, limit: usize) -> Result<Vec<SearchHit>, String> {
        if let Some((mq, ml, mg, hits)) = &*self.memo.borrow() {
            if mq == q && *ml == limit && *mg == self.generation {
                return Ok(hits.clone());
            }
        }
        let rank = match self.spec.as_ref().is_some_and(|s| s.recent()) {
            true => Rank::Recent,
            false => Rank::Relevance,
        };
        let hits: Vec<SearchHit> = self
            .index
            .query(q, &QueryOpts { limit, rank })
            .map_err(|e| e.to_string())?
            .into_iter()
            .map(|h| SearchHit {
                doc: h.doc,
                id: h.id,
                score: h.score,
                snippet: h.snippet,
            })
            .collect();
        *self.memo.borrow_mut() = Some((q.to_string(), limit, self.generation, hits.clone()));
        Ok(hits)
    }

    /// Re-read `index.lua` if its text moved. `Ok(true)` when the spec changed, which makes
    /// every covered doc stale. A spec that fails to load keeps the last good one running.
    fn sync_spec(&mut self, src: Option<String>) -> Result<bool, String> {
        if src == self.spec_src {
            return Ok(false);
        }
        let spec = src.as_deref().map(IndexSpec::load).transpose()?;
        self.spec_src = src;
        self.spec = spec;
        Ok(true)
    }

    fn covered(&self, names: impl IntoIterator<Item = String>) -> Vec<String> {
        match &self.spec {
            Some(spec) => names.into_iter().filter(|n| spec.covers(n)).collect(),
            None => Vec::new(),
        }
    }

    /// Plan one doc against its stored fingerprints and stage the result; [`Self::commit`]
    /// lands a batch. Per-record failures come back as lines for the app's console.
    fn apply(&mut self, name: &str, doc: &LoroDoc) -> Result<Vec<String>, String> {
        let Some(spec) = &self.spec else {
            return Ok(Vec::new());
        };
        let prev = self.index.prints(name).map_err(|e| e.to_string())?;
        let plan = spec.plan(name, doc, prev.as_deref())?;
        self.fields_runs += plan.fields_runs;
        let notes = plan.errors.into_iter().map(|e| format!("index.lua: {e}")).collect();
        for (id, f) in plan.upserts {
            self.index
                .upsert(&Record {
                    doc: name.to_string(),
                    id,
                    title: f.title,
                    body: f.body,
                    facets: f.facets,
                    time: f.time,
                })
                .map_err(|e| e.to_string())?;
            self.dirty = true;
        }
        for id in plan.deletes {
            self.index.delete(name, &id).map_err(|e| e.to_string())?;
            self.dirty = true;
        }
        if prev.as_deref() != Some(&plan.prints[..]) {
            self.index.set_prints(name, plan.prints);
            self.dirty = true;
        }
        Ok(notes)
    }

    fn commit(&mut self) -> Result<(), String> {
        if std::mem::take(&mut self.dirty) {
            self.index.commit().map_err(|e| e.to_string())?;
            self.generation += 1;
        }
        Ok(())
    }
}

/// Plan and stage `names`, reading each from the live core when the app has it open — no copy
/// of a large doc per flush — and from the vault's snapshot when it does not. One commit.
fn reindex<M: 'static>(shared: &Shared, app: &LuaApp<M>, names: Vec<String>) -> Result<(), String> {
    let home = shared.borrow().home.clone();
    let mut notes = Vec::new();
    // Bound first: in the `for` header the borrow would live for the whole loop.
    let names = shared.borrow().covered(names);
    for name in names {
        let live = app.with_doc(&name, |doc| shared.borrow_mut().apply(&name, doc));
        match live {
            Some(r) => notes.extend(r?),
            None => {
                let Some((vault, ws, item)) = &home else { continue };
                let Some(bytes) = vault.get_doc(ws, item, &name).map_err(|e| e.to_string())? else {
                    continue;
                };
                let doc = LoroDoc::new();
                doc.import(&bytes).map_err(|e| e.to_string())?;
                notes.extend(shared.borrow_mut().apply(&name, &doc)?);
            }
        }
    }
    shared.borrow_mut().commit()?;
    notes.into_iter().for_each(|n| app.note(n));
    Ok(())
}

/// Every covered doc the item has — open now, or only in the vault. Fingerprints make this
/// cheap when nothing changed, which is what lets it run at every open and every spec edit.
fn catch_up<M: 'static>(shared: &Shared, app: &LuaApp<M>) -> Result<(), String> {
    let mut names = app.open_doc_names();
    if let Some((vault, ws, item)) = &shared.borrow().home {
        names.extend(vault.doc_names(ws, item).map_err(|e| e.to_string())?);
    }
    names.sort();
    names.dedup();
    reindex(shared, app, names)
}

/// Wire an opened app to its index: load `index.lua`, catch up, and answer `search.query`.
pub fn attach<M: 'static>(shared: &Shared, app: &mut LuaApp<M>) -> Result<(), String> {
    if let Err(e) = shared.borrow_mut().sync_spec(app.read_source_file("index.lua")) {
        app.note(e);
    }
    catch_up(shared, app)?;
    let weak = Rc::downgrade(shared);
    let f: SearchFn = Rc::new(move |q, limit| match weak.upgrade() {
        Some(s) => s.borrow().query(q, limit),
        None => Err("the index was closed".into()),
    });
    app.set_search(Some(f));
    Ok(())
}

/// After a flush: re-plan the docs it saved, or every covered doc if `index.lua` changed.
pub fn index_dirty<M: 'static>(shared: &Shared, app: &LuaApp<M>, dirtied: &[String]) -> Result<(), String> {
    let changed = shared.borrow_mut().sync_spec(app.read_source_file("index.lua"));
    match changed {
        Err(e) => {
            app.note(e);
            Ok(())
        }
        Ok(true) => catch_up(shared, app),
        Ok(false) => reindex(shared, app, dirtied.to_vec()),
    }
}

#[cfg(test)]
mod tests;
