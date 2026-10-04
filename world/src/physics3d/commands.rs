use super::*;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Set3d {
    pub pos: Option<[f32; 3]>,
    pub rotation: Option<[f32; 4]>,
    pub velocity: Option<[f32; 3]>,
    /// Degrees per second about world x/y/z; native inspection remains radians per second.
    pub spin: Option<[f32; 3]>,
}

impl Physics3d {
    pub fn set(&mut self, handle: RigidBodyHandle, to: &Set3d) -> Result<(), InvalidBody> {
        let bounded = |v: f32| v.is_finite() && v.abs() <= LIMIT;
        let valid = [&to.pos, &to.velocity, &to.spin]
            .into_iter()
            .all(|v| v.is_none_or(|v| v.into_iter().all(bounded)))
            && to.rotation.is_none_or(Self::valid_rotation);
        let body = &self.world.bodies[handle];
        if !valid || (!body.is_dynamic() && (to.velocity.is_some() || to.spin.is_some())) {
            return Err(InvalidBody);
        }
        let wake_others = body.is_fixed()
            && (to.pos.is_some() || to.rotation.is_some())
            && !self.world.colliders[body.colliders()[0]].is_sensor();
        let body = &mut self.world.bodies[handle];
        if let Some(pos) = to.pos {
            body.set_translation(Vector::from_array(pos), true);
        }
        if let Some(rotation) = to.rotation {
            body.set_rotation(Rotation::from_array(rotation).normalize(), true);
        }
        if let Some(velocity) = to.velocity {
            body.set_linvel(Vector::from_array(velocity), true);
        }
        if let Some(spin) = to.spin {
            body.set_angvel(Vector::from_array(spin.map(f32::to_radians)), true);
        }
        // A moved fixed solid must not leave a resting dynamic body asleep through new contact.
        if wake_others {
            for (_, body) in self.world.bodies.iter_mut() {
                if body.is_dynamic() {
                    body.wake_up(true);
                }
            }
        }
        Ok(())
    }
}
