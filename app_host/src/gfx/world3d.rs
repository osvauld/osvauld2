//! Retained native worlds are app-local; staged VMs collect recipes without touching the solver.
use std::cell::{Cell, RefCell};
use std::collections::HashSet;
use std::rc::Rc;
use std::sync::Arc;

use mlua::{Error, Lua, Table, UserData, UserDataMethods};
use runtime::scene3d::Scene3d;
use world::world3d::{WorldRecipes3d, Worlds3d};

mod handle;
mod recipe;

#[derive(Clone, Default)]
pub(crate) struct Host {
    worlds: Rc<RefCell<Worlds3d>>,
    recipes: Rc<RefCell<WorldRecipes3d>>,
    seen: Rc<RefCell<HashSet<String>>>,
    ready: Rc<Cell<bool>>,
    pub(crate) commanding: Rc<Cell<bool>>,
}

impl Host {
    pub(crate) fn staged(&self) -> Self {
        Self {
            worlds: self.worlds.clone(),
            ..Self::default()
        }
    }

    pub(crate) fn commit(&self) -> mlua::Result<()> {
        self.worlds
            .borrow_mut()
            .reconcile(self.recipes.borrow().clone())
            .map_err(Error::external)?;
        self.ready.set(true);
        Ok(())
    }

    pub(crate) fn inspect(
        &self,
    ) -> std::collections::BTreeMap<String, world::world3d::WorldInspection3d> {
        let worlds = self.worlds.borrow();
        self.recipes
            .borrow()
            .keys()
            .filter_map(|id| worlds.get(id).map(|world| (id.clone(), world.inspect())))
            .collect()
    }

    pub(crate) fn begin_view(&self) {
        self.seen.borrow_mut().clear();
    }

    pub(crate) fn advance(&self, elapsed: f64, active: bool) -> Result<bool, String> {
        let recipes = self.recipes.borrow();
        let seen = self.seen.borrow();
        let mut worlds = self.worlds.borrow_mut();
        let mut ticking = false;
        for id in recipes.keys() {
            if let Some(world) = worlds.get_mut(id) {
                let running = active && seen.contains(id) && world.needs_ticks();
                world.advance(elapsed, running).map_err(|e| e.to_string())?;
                ticking |= running && world.needs_ticks();
            }
        }
        Ok(ticking)
    }

    pub(crate) fn install(&self, lua: &Lua, viewing: Rc<Cell<bool>>) -> mlua::Result<()> {
        let host = self.clone();
        // sandboxed_vm froze the shared globals. Extend an app-local table, never thaw it.
        let base: Table = lua.globals().get("gfx")?;
        let gfx = lua.create_table()?;
        for pair in base.pairs::<mlua::Value, mlua::Value>() {
            let (key, value) = pair?;
            gfx.set(key, value)?;
        }
        gfx.set(
            "world3d",
            lua.create_function(move |lua, spec: Table| {
                if viewing.get() || host.ready.get() {
                    return Err(Error::runtime(
                        "gfx.world3d is only allowed during module initialization",
                    ));
                }
                let (id, scene, bodies) = recipe::parse(spec)?;
                let mut recipes = host.recipes.borrow().clone();
                if recipes.insert(id.clone(), bodies).is_some() {
                    return Err(Error::runtime(format!("repeated 3D world id: {id}")));
                }
                host.worlds
                    .borrow()
                    .validate_reconcile(&recipes)
                    .map_err(Error::external)?;
                let handle = lua.create_userdata(handle::WorldHandle {
                    id,
                    scene,
                    host: host.clone(),
                })?;
                *host.recipes.borrow_mut() = recipes;
                Ok(handle)
            })?,
        )?;
        gfx.set_readonly(true);
        lua.globals().set("gfx", gfx)
    }
}

pub(crate) struct SceneHandle {
    id: String,
    scene: Arc<Scene3d>,
    host: Host,
}
impl UserData for SceneHandle {}

impl SceneHandle {
    pub(crate) fn resolve(&self) -> mlua::Result<(Arc<Scene3d>, bool)> {
        let worlds = self.host.worlds.borrow();
        let world = worlds
            .get(&self.id)
            .ok_or_else(|| Error::runtime("3D world is not active"))?;
        let scene = world.resolved_scene(&self.scene).map_err(Error::external)?;
        self.host.seen.borrow_mut().insert(self.id.clone());
        Ok((scene, world.needs_ticks()))
    }
}
