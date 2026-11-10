use super::*;
use std::collections::BTreeMap;

impl World3d {
    pub fn validate_recipe(&self, recipe: &WorldRecipe3d) -> Result<(), World3dError> {
        Self::validate_gravity(recipe.gravity)?;
        self.validate_reconcile(&recipe.bodies)?;
        let bodies: BTreeMap<_, _> = recipe.bodies.iter().map(|b| (&b.id, b)).collect();
        for id in recipe.properties.keys() {
            if !bodies.contains_key(id) {
                return Err(World3dError::Missing(id.clone()));
            }
        }
        for body in &recipe.bodies {
            if self.physics.wheel_specs().any(|w| w.id == body.id) {
                return Err(World3dError::Changed(body.id.clone()));
            }
            let default = BodyProperties3d::default();
            let p = recipe.properties.get(&body.id).unwrap_or(&default);
            if !p.valid(body.dynamic) {
                return Err(World3dError::Properties(body.id.clone()));
            }
            if self.ids.contains_key(&body.id)
                && self.properties.get(&body.id).unwrap_or(&default) != p
            {
                return Err(World3dError::Changed(body.id.clone()));
            }
        }
        if recipe.wheels.len() > 128 {
            return Err(World3dError::Wheel("at most 128 wheels per world".into()));
        }
        let mut ids: HashSet<_> = recipe.bodies.iter().map(|b| b.id.clone()).collect();
        let mut counts = BTreeMap::new();
        for spec in &recipe.wheels {
            if self.ids.contains_key(&spec.id) {
                return Err(World3dError::Changed(spec.id.clone()));
            }
            let body = bodies
                .get(&spec.body)
                .ok_or_else(|| World3dError::Missing(spec.body.clone()))?;
            let count = counts.entry(&spec.body).or_insert(0);
            *count += 1;
            if !body.dynamic
                || body.sensor
                || !spec.valid()
                || !ids.insert(spec.id.clone())
                || *count > 8
            {
                return Err(World3dError::Wheel(spec.id.clone()));
            }
            if self
                .physics
                .wheel_specs()
                .any(|old| old.id == spec.id && old != spec)
            {
                return Err(World3dError::Changed(spec.id.clone()));
            }
        }
        Ok(())
    }
    pub fn reconcile_recipe(&mut self, recipe: WorldRecipe3d) -> Result<(), World3dError> {
        self.validate_recipe(&recipe)?;
        self.reconcile(recipe.bodies)?;
        self.properties.retain(|id, _| self.ids.contains_key(id));
        for (id, e) in &self.ids {
            let props = recipe.properties.get(id).cloned().unwrap_or_default();
            if !self.properties.contains_key(id) {
                self.physics
                    .properties(self.ecs.get::<Body>(*e).unwrap().handle, &props);
                self.properties.insert(id.clone(), props);
            }
        }
        let handles = self
            .ids
            .iter()
            .map(|(id, e)| (id.clone(), self.ecs.get::<Body>(*e).unwrap().handle))
            .collect();
        self.physics.reconcile_wheels(recipe.wheels, &handles);
        self.set_gravity(recipe.gravity)?;
        Ok(())
    }
    pub fn set_wheels(&mut self, commands: &[(String, WheelInput3d)]) -> Result<(), World3dError> {
        if self.physics.set_wheels(commands) {
            Ok(())
        } else {
            Err(World3dError::Wheel("invalid actuator batch".into()))
        }
    }
}
