//! `ui.group` in the UI tree: a group whose signals did not change hands back the `El` it built
//! last time, with the handlers it registered, and its function does not run
//! (`docs/design/ui-reuse.md` step 2). World groups are the other kind, in `gfx`'s world path.
//!
//! A group's output is everything under it, so what a nested group read, it read: the nested
//! reads are kept with the outer group's. A group that cannot say when it changed — it read no
//! signal, holds a world (whose frame moves on its own) or a retained 3D scene, or an element
//! failed — is not kept, and neither is any group around it.

use std::collections::HashMap;
use std::rc::Rc;
use std::cell::RefCell;

use mlua::{Function, Table, Value};
use runtime::El;

use crate::signal::{self, Read};
use crate::{Ctx, Handlers, reason, walk};

/// What a group built and what it needs to be handed back.
pub(crate) struct Kept<M> {
    el: El<M>,
    handlers: Handlers,
    /// Every group below it, so they are not forgotten while this one is kept.
    nested: Vec<(String, String)>,
}

/// Retained per VM, by group id: a reload's new signals are not the old ones.
pub(crate) type KeptGroups<M> = Rc<RefCell<HashMap<String, Kept<M>>>>;

/// What the groups being built so far have collected for the one around them.
#[derive(Default)]
pub(crate) struct Scope {
    reads: Vec<Read>,
    groups: Vec<(String, String)>,
    /// Something below cannot be kept, so nothing around it can be.
    pub(crate) volatile: bool,
}

fn merge<M>(context: &mut Ctx<M>, handlers: &Handlers) -> mlua::Result<()> {
    for (key, f) in handlers {
        if context.handlers.insert(key.clone(), f.clone()).is_some() {
            return Err(mlua::Error::runtime(format!(
                "duplicate id {}: another element already has {}",
                key.id, key.name
            )));
        }
    }
    Ok(())
}

pub(crate) fn build_group<M: Clone + 'static>(node: &Table, context: &mut Ctx<M>) -> mlua::Result<El<M>> {
    let id: String = node.get("id")?;
    let key = (String::new(), id.clone());
    if !context.groups_seen.insert(key.clone()) {
        return Err(mlua::Error::runtime(format!(
            "ui.group {id:?} appears twice in one view: group ids are unique in the UI tree"
        )));
    }
    let unchanged = context.groups.borrow().get(&key).is_some_and(|r| signal::unchanged(r));
    if unchanged {
        let store = context.kept.clone();
        let kept = store.borrow();
        if let Some(kept) = kept.get(&id) {
            merge(context, &kept.handlers)?;
            context.groups_seen.extend(kept.nested.iter().cloned());
            context.scope.groups.push(key.clone());
            context.scope.groups.extend(kept.nested.iter().cloned());
            if let Some(reads) = context.groups.borrow().get(&key) {
                context.scope.reads.extend(reads.iter().cloned());
            }
            return Ok(kept.el.clone());
        }
    }

    let describe: Function = node.get("describe")?;
    let fail = |why: String| mlua::Error::runtime(format!("group {id:?}: {why}"));
    let viewing = context.viewing.replace(true);
    let (tree, own) = context.tracker.reading(|| describe.call::<Value>(()));
    context.viewing.set(viewing);
    let tree = match tree.map_err(|e| fail(reason(&e)))? {
        Value::Table(t) => t,
        other => return Err(fail(format!("its function must return an element, got {}", other.type_name()))),
    };

    let outer_handlers = std::mem::take(context.handlers);
    let outer_scope = std::mem::take(&mut context.scope);
    let errors = context.errors.len();
    let built = walk(tree, context);
    let scope = std::mem::replace(&mut context.scope, outer_scope);
    let handlers = std::mem::replace(context.handlers, outer_handlers);
    let el = built?;
    merge(context, &handlers)?;

    let keepable = !own.is_empty() && !scope.volatile && context.errors.len() == errors;
    context.scope.groups.push(key.clone());
    context.scope.groups.extend(scope.groups.iter().cloned());
    context.scope.volatile |= !keepable;
    let mut reads = own;
    reads.extend(scope.reads);
    if keepable {
        context.scope.reads.extend(reads.iter().cloned());
        context.kept.borrow_mut().insert(id, Kept { el: el.clone(), handlers, nested: scope.groups });
        context.groups.borrow_mut().insert(key, reads);
    } else {
        context.kept.borrow_mut().remove(&id);
        context.groups.borrow_mut().insert(key, Vec::new());
    }
    Ok(el)
}
