use super::*;

#[derive(Clone, Debug, PartialEq)]
pub struct BodyProperties3d {
    pub mass: Option<f32>,
    pub center: [f32; 3],
    pub friction: f32,
    pub restitution: f32,
}
impl Default for BodyProperties3d {
    fn default() -> Self {
        Self {
            mass: None,
            center: [0.0; 3],
            friction: 0.5,
            restitution: 0.35,
        }
    }
}
impl BodyProperties3d {
    pub fn valid(&self, dynamic: bool) -> bool {
        self.mass
            .is_none_or(|m| dynamic && m.is_finite() && (0.001..=100_000.0).contains(&m))
            && self.center.iter().all(|v| v.is_finite() && v.abs() <= 10.0)
            && (dynamic || self.center == [0.0; 3])
            && self.friction.is_finite()
            && (0.0..=5.0).contains(&self.friction)
            && self.restitution.is_finite()
            && (0.0..=1.0).contains(&self.restitution)
    }
}
impl Physics3d {
    pub(crate) fn properties(&mut self, handle: RigidBodyHandle, p: &BodyProperties3d) {
        let collider = self.world.bodies[handle].colliders()[0];
        let c = &mut self.world.colliders[collider];
        c.set_friction(p.friction);
        c.set_restitution(p.restitution);
        if p.mass.is_some() || p.center != [0.0; 3] {
            let own = c.shape().mass_properties(1.0);
            let mass = p.mass.unwrap_or(own.mass());
            c.set_mass_properties(MassProperties::new(
                Vector::from_array(p.center),
                mass,
                own.principal_inertia() * (mass / own.mass()),
            ));
            self.world.bodies[handle]
                .recompute_mass_properties_from_colliders(&self.world.colliders);
        }
    }
    pub(crate) fn mass(&self, handle: RigidBodyHandle) -> f32 {
        self.world.bodies[handle].mass()
    }
}
