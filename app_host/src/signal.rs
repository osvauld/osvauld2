//! `signal(value, name)`: app state whose value is frozen, so a write that skips `set` and
//! `update` errors instead of going unseen. Groups will re-run on a signal's change
//! (`docs/design/signals.md`); this is the value half.
use std::cell::Cell;
use std::rc::Rc;

use mlua::{AnyUserData, Error, Function, Lua, MetaMethod, Table, UserData, UserDataMethods, Value};

struct Signal {
    name: String,
    value: Value,
    /// Bumped by every change; what a group that read it compares against.
    version: u64,
    /// Inside this signal's own `update`: a nested one would be overwritten when it returns.
    updating: bool,
    viewing: Rc<Cell<bool>>,
    luau: Rc<Luau>,
}

/// Copying and freezing run in Luau's own `table.clone` and `table.freeze`: walking a table
/// from Rust costs a call per entry, 16 times slower for 5000.
struct Luau {
    freeze: Function,
    clone: Function,
}

/// Makes `v` and every table under it read-only. A frozen table is skipped, so what an `update`
/// kept is not walked again (and a cycle ends); a table with a metatable belongs to something
/// else (a doc mirror, a `doc.map` tag) and is left alone.
const FREEZE: &str = r#"
local isfrozen, freeze, getmetatable, type, next = table.isfrozen, table.freeze, getmetatable, type, next
local function deep(v)
  if type(v) ~= "table" or isfrozen(v) or getmetatable(v) ~= nil then return end
  freeze(v)
  for k, x in next, v do
    if type(k) == "table" and not isfrozen(k) then deep(k) end
    if type(x) == "table" and not isfrozen(x) then deep(x) end
  end
end
return deep
"#;

impl Signal {
    fn writable(&self, how: &str) -> mlua::Result<()> {
        match self.viewing.get() {
            true => Err(Error::runtime(format!(
                "signal {:?}: {how} only in a handler; view describes",
                self.name
            ))),
            false => Ok(()),
        }
    }
}

impl UserData for Signal {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_meta_method(MetaMethod::Call, |_, this, ()| Ok(this.value.clone()));
        methods.add_meta_method(MetaMethod::ToString, |_, this, ()| {
            Ok(format!("signal {:?}", this.name))
        });
        methods.add_method_mut("set", |_, this, value: Value| {
            this.writable("set")?;
            if value == this.value {
                return Ok(()); // the same value: nothing changed, nothing re-runs
            }
            this.luau.freeze.call::<()>(&value)?;
            this.value = value;
            this.version += 1;
            Ok(())
        });
        // Borrowed only around the call: `f` may read this signal.
        methods.add_function("update", |_, (ud, f): (AnyUserData, Function)| {
            let (value, name, luau) = {
                let mut this = ud.borrow_mut::<Signal>()?;
                this.writable("update")?;
                if this.updating {
                    return Err(Error::runtime(format!(
                        "signal {:?}: update inside its own update: the outer one would overwrite it",
                        this.name
                    )));
                }
                this.updating = true;
                (this.value.clone(), this.name.clone(), this.luau.clone())
            };
            let new = (|| match &value {
                Value::Table(t) => {
                    // A writable copy whose entries (frozen) are shared with the old value.
                    let draft: Table = luau.clone.call(t)?;
                    match f.call::<Value>(draft.clone())? {
                        Value::Nil => Ok(Value::Table(draft)),
                        returned => Ok(returned),
                    }
                }
                other => match f.call::<Value>(other.clone())? {
                    Value::Nil => Err(Error::runtime(format!(
                        "signal {name:?}: update's function must return the new value"
                    ))),
                    returned => Ok(returned),
                },
            })();
            let new = new.and_then(|new| luau.freeze.call::<()>(&new).map(|()| new));
            let mut this = ud.borrow_mut::<Signal>()?;
            this.updating = false;
            this.value = new?;
            this.version += 1;
            Ok(())
        });
    }
}

/// How many times a signal has changed. Groups will compare it (signals step 2).
#[cfg(test)]
pub(crate) fn version(signal: &AnyUserData) -> mlua::Result<u64> {
    Ok(signal.borrow::<Signal>()?.version)
}

pub(crate) fn install(lua: &Lua, viewing: Rc<Cell<bool>>) -> mlua::Result<()> {
    let tables: Table = lua.globals().get("table")?;
    let luau = Rc::new(Luau {
        freeze: lua.load(FREEZE).set_name("signal freeze").eval()?,
        clone: tables.get("clone")?,
    });
    let signal = lua.create_function(move |_, (value, name): (Value, Option<String>)| {
        let name = name
            .filter(|n| !n.is_empty())
            .ok_or_else(|| Error::runtime("signal needs a name: signal(value, \"coins\")"))?;
        luau.freeze.call::<()>(&value)?;
        let (viewing, luau) = (viewing.clone(), luau.clone());
        Ok(Signal { name, value, version: 0, updating: false, viewing, luau })
    })?;
    lua.globals().set("signal", signal)
}
