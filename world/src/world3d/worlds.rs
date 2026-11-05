//! App-local retained worlds. A staged source recipe is accepted as one validated batch.
use std::collections::BTreeMap;

use super::{EntitySpec3d, World3d, World3dError};

#[derive(Clone, Debug, PartialEq)]
pub struct WorldRecipe3d {
    pub wheels: Vec<super::WheelSpec3d>,
    pub properties: BTreeMap<String, super::BodyProperties3d>,
    pub bodies: Vec<EntitySpec3d>,
    pub gravity: [f32; 3],
}

impl Default for WorldRecipe3d {
    fn default() -> Self {
        Self {
            wheels: Vec::new(),
            properties: BTreeMap::new(),
            bodies: Vec::new(),
            gravity: crate::physics3d::DEFAULT_GRAVITY,
        }
    }
}

impl From<Vec<EntitySpec3d>> for WorldRecipe3d {
    fn from(bodies: Vec<EntitySpec3d>) -> Self {
        Self {
            bodies,
            ..Self::default()
        }
    }
}

pub type WorldRecipes3d = BTreeMap<String, WorldRecipe3d>;

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
        for (id, recipe) in recipes {
            if id.is_empty() || id.len() > 128 {
                return Err(Worlds3dError::Id(id.clone()));
            }
            let result = match self.worlds.get(id) {
                Some(world) => world.validate_recipe(recipe),
                None => World3d::default().validate_recipe(recipe),
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
        for (id, recipe) in recipes {
            let world = self.worlds.entry(id).or_default();
            world
                .reconcile_recipe(recipe)
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
