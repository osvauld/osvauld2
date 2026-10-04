//! App-local retained worlds. A staged source recipe is accepted as one validated batch.
use std::collections::BTreeMap;

use super::{EntitySpec3d, World3d, World3dError};

pub type WorldRecipes3d = BTreeMap<String, Vec<EntitySpec3d>>;

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum Worlds3dError {
    #[error("an app is limited to 8 retained 3D worlds")]
    Budget,
    #[error("3D world id is empty or too long: {0}")]
    Id(String),
    #[error("3D world {id}: {source}")]
    Recipe {
        id: String,
        #[source]
        source: World3dError,
    },
}

#[derive(Default)]
pub struct Worlds3d {
    worlds: BTreeMap<String, World3d>,
}

impl Worlds3d {
    pub fn validate_reconcile(&self, recipes: &WorldRecipes3d) -> Result<(), Worlds3dError> {
        if recipes.len() > 8 {
            return Err(Worlds3dError::Budget);
        }
        for (id, specs) in recipes {
            if id.is_empty() || id.len() > 128 {
                return Err(Worlds3dError::Id(id.clone()));
            }
            let result = match self.worlds.get(id) {
                Some(world) => world.validate_reconcile(specs),
                None => World3d::default().validate_reconcile(specs),
            };
            result.map_err(|source| Worlds3dError::Recipe {
                id: id.clone(),
                source,
            })?;
        }
        Ok(())
    }

    /// Preflight every recipe before updating any live world or dropping omitted worlds.
    /// The caller must invoke this only after the staged VM and trial view both succeed.
    pub fn reconcile(&mut self, recipes: WorldRecipes3d) -> Result<(), Worlds3dError> {
        self.validate_reconcile(&recipes)?;
        self.worlds.retain(|id, _| recipes.contains_key(id));
        for (id, specs) in recipes {
            self.worlds
                .entry(id)
                .or_default()
                .reconcile(specs)
                .expect("all recipes validated before mutation");
        }
        Ok(())
    }

    pub fn get(&self, id: &str) -> Option<&World3d> {
        self.worlds.get(id)
    }

    pub fn get_mut(&mut self, id: &str) -> Option<&mut World3d> {
        self.worlds.get_mut(id)
    }
}

#[cfg(test)]
mod tests;
