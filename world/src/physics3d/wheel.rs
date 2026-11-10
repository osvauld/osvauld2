//! Ray-supported wheel components on ordinary rigid bodies. Inputs are SI actuator commands.
use super::*;
use rapier3d::control::{DynamicRayCastVehicleController, Wheel, WheelTuning};
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq)]
pub struct WheelSpec3d {
    pub id: String,
    pub body: String,
    pub at: [f32; 3],
    pub radius: f32,
    pub width: f32,
    pub rest: f32,
    pub travel: f32,
    pub stiffness: f32,
    pub damping: f32,
    pub grip: f32,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WheelMotor3d {
    pub velocity: f32,
    pub max_torque: f32,
}
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct WheelInput3d {
    pub torque: f32,
    pub brake: f32,
    pub steering: f32,
    pub motor: Option<WheelMotor3d>,
}
impl WheelInput3d {
    pub fn valid(self) -> bool {
        self.torque.is_finite()
            && self.torque.abs() <= 100_000.0
            && self.brake.is_finite()
            && (0.0..=100_000.0).contains(&self.brake)
            && self.steering.is_finite()
            && self.steering.abs() <= std::f32::consts::PI
            && self.motor.is_none_or(|m| {
                self.torque == 0.0
                    && m.velocity.is_finite()
                    && m.velocity.abs() <= 10_000.0
                    && m.max_torque.is_finite()
                    && (0.0..=100_000.0).contains(&m.max_torque)
            })
    }
}
#[derive(Clone, Debug, PartialEq)]
pub struct WheelState3d {
    pub spec: WheelSpec3d,
    pub input: WheelInput3d,
    pub position: [f32; 3],
    pub rotation: [f32; 4],
    pub contact: bool,
    pub suspension_length: f32,
    pub force: f32,
    pub steering: f32,
    pub angle: f32,
}
pub(super) struct WheelGroup {
    pub specs: Vec<WheelSpec3d>,
    controller: DynamicRayCastVehicleController,
    inputs: Vec<WheelInput3d>,
    colliders: Vec<ColliderHandle>,
}
impl WheelSpec3d {
    pub fn valid(&self) -> bool {
        let range = |v: f32, lo: f32, hi: f32| v.is_finite() && (lo..=hi).contains(&v);
        !self.id.is_empty()
            && self.id.len() <= 128
            && self.at.iter().all(|v| range(*v, -20.0, 20.0))
            && range(self.radius, 0.05, 2.0)
            && range(self.width, 0.02, 2.0)
            && range(self.rest, 0.02, 2.0)
            && range(self.travel, 0.0, self.rest)
            && range(self.stiffness, 1.0, 1_000_000.0)
            && range(self.damping, 0.0, 100_000.0)
            && range(self.grip, 0.0, 5.0)
    }
}
fn controller(
    specs: &[WheelSpec3d],
    body: RigidBodyHandle,
    mass: f32,
) -> DynamicRayCastVehicleController {
    let mut c = DynamicRayCastVehicleController::new(body);
    c.index_forward_axis = 2;
    for w in specs {
        let tuning = WheelTuning {
            // Rapier's coefficients are mass-normalized; the public component uses SI.
            suspension_stiffness: w.stiffness / mass,
            suspension_compression: w.damping / mass,
            suspension_damping: w.damping / mass,
            max_suspension_travel: w.travel,
            friction_slip: w.grip,
            side_friction_stiffness: 1.0,
            max_suspension_force: w.stiffness * w.travel + w.damping * 20.0,
        };
        c.add_wheel(
            Vector::from_array(w.at),
            -Vector::Y,
            -Vector::X,
            w.rest,
            w.radius,
            &tuning,
        );
    }
    c
}
impl Physics3d {
    pub(crate) fn reconcile_wheels(
        &mut self,
        specs: Vec<WheelSpec3d>,
        handles: &BTreeMap<String, RigidBodyHandle>,
    ) {
        let mut groups: BTreeMap<String, Vec<WheelSpec3d>> = BTreeMap::new();
        for s in specs {
            groups.entry(s.body.clone()).or_default().push(s);
        }
        self.wheel_groups.retain(|body, g| {
            let keep = groups.get(body) == Some(&g.specs);
            if !keep {
                for c in &g.colliders {
                    self.world.remove_collider(*c);
                }
            }
            keep
        });
        for (id, specs) in groups {
            if self.wheel_groups.contains_key(&id) {
                continue;
            }
            let parent = handles[&id];
            self.world.bodies[parent].wake_up(true);
            let c = controller(&specs, parent, self.world.bodies[parent].mass());
            let colliders = specs
                .iter()
                .map(|w| {
                    // Keep obstacle contact just inside the ray's tire-ground support plane.
                    self.world.insert_collider(
                        ColliderBuilder::cylinder(w.width / 2.0, w.radius * 0.94)
                            .position(wheel_collider_pose(w, w.rest, 0.0))
                            .density(0.0)
                            .friction(0.0)
                            .restitution(0.0),
                        Some(parent),
                    )
                })
                .collect();
            let inputs = vec![WheelInput3d::default(); specs.len()];
            self.wheel_groups.insert(
                id,
                WheelGroup {
                    specs,
                    controller: c,
                    inputs,
                    colliders,
                },
            );
        }
    }
    pub(super) fn step_wheels(&mut self) {
        for g in self.wheel_groups.values_mut() {
            let body = &self.world.bodies[g.controller.chassis];
            if body.is_sleeping() {
                continue;
            }
            let mass = body.mass();
            let count = g.specs.len() as f32;
            for ((wheel, spec), input) in g
                .controller
                .wheels_mut()
                .iter_mut()
                .zip(&g.specs)
                .zip(&g.inputs)
            {
                wheel.steering = input.steering;
                let forward =
                    body.rotation() * Rotation::from_rotation_y(input.steering) * Vector::Z;
                let speed = body.linvel().dot(forward);
                let motor_force = input.motor.map_or(0.0, |motor| {
                    let desired = (motor.velocity * spec.radius - speed) * mass / (count * STEP);
                    desired.clamp(
                        -motor.max_torque / spec.radius,
                        motor.max_torque / spec.radius,
                    )
                });
                let applied = input.torque / spec.radius + motor_force;
                let brake_limit = input.brake / spec.radius;
                // A friction brake opposes motion/drive up to its torque bound. Combining
                // these physically avoids inheriting the backend's engine-over-brake policy.
                let friction =
                    (speed * mass / (count * STEP) + applied).clamp(-brake_limit, brake_limit);
                wheel.engine_force = applied - friction;
                wheel.brake = 0.0;
            }
            let filter = QueryFilter::default()
                .exclude_rigid_body(g.controller.chassis)
                .exclude_sensors();
            let queries = self.world.broad_phase.as_query_pipeline_mut(
                self.world.narrow_phase.query_dispatcher(),
                &mut self.world.bodies,
                &mut self.world.colliders,
                filter,
            );
            g.controller.update_vehicle(STEP, queries);
            for ((wheel, spec), handle) in
                g.controller.wheels().iter().zip(&g.specs).zip(&g.colliders)
            {
                let length = if wheel.raycast_info().is_in_contact {
                    wheel.raycast_info().suspension_length
                } else {
                    spec.rest
                };
                self.world.colliders[*handle].set_position_wrt_parent(wheel_collider_pose(
                    spec,
                    length,
                    wheel.steering,
                ));
            }
        }
    }
    pub(crate) fn set_wheels(&mut self, commands: &[(String, WheelInput3d)]) -> bool {
        let mut seen = std::collections::HashSet::new();
        if commands.len() > 128
            || commands.iter().any(|(id, input)| {
                !input.valid() || !seen.insert(id) || !self.wheel_specs().any(|s| &s.id == id)
            })
        {
            return false;
        }
        for (id, input) in commands {
            for g in self.wheel_groups.values_mut() {
                if let Some(i) = g.specs.iter().position(|s| &s.id == id) {
                    if g.inputs[i] != *input || input.torque != 0.0 || input.motor.is_some() {
                        self.world.bodies[g.controller.chassis].wake_up(true);
                    }
                    g.inputs[i] = *input;
                    break;
                }
            }
        }
        true
    }
    pub(super) fn reset_wheels(&mut self, body: RigidBodyHandle) {
        for g in self
            .wheel_groups
            .values_mut()
            .filter(|g| g.controller.chassis == body)
        {
            g.controller = controller(&g.specs, body, self.world.bodies[body].mass());
            g.inputs.fill(WheelInput3d::default());
            for (spec, handle) in g.specs.iter().zip(&g.colliders) {
                self.world.colliders[*handle]
                    .set_position_wrt_parent(wheel_collider_pose(spec, spec.rest, 0.0));
            }
        }
    }
    pub(crate) fn wheel_pose(&self, id: &str) -> Option<([f32; 3], [f32; 4])> {
        self.wheel_groups.values().find_map(|g| {
            let i = g.specs.iter().position(|s| s.id == id)?;
            let (p, q, _) = wheel_pose(
                &self.world.bodies[g.controller.chassis],
                &g.controller.wheels()[i],
                &g.specs[i],
            );
            Some((p, q))
        })
    }
    pub fn inspect_wheels(&self) -> Vec<WheelState3d> {
        let mut states = Vec::new();
        for g in self.wheel_groups.values() {
            for ((w, s), input) in g.controller.wheels().iter().zip(&g.specs).zip(&g.inputs) {
                let (position, rotation, suspension_length) =
                    wheel_pose(&self.world.bodies[g.controller.chassis], w, s);
                states.push(WheelState3d {
                    spec: s.clone(),
                    input: *input,
                    position,
                    rotation,
                    suspension_length,
                    contact: w.raycast_info().is_in_contact,
                    force: w.wheel_suspension_force,
                    steering: w.steering,
                    angle: w.rotation,
                });
            }
        }
        states.sort_by(|a, b| a.spec.id.cmp(&b.spec.id));
        states
    }
    pub(crate) fn wheel_specs(&self) -> impl Iterator<Item = &WheelSpec3d> {
        self.wheel_groups.values().flat_map(|g| g.specs.iter())
    }
    pub(super) fn remove_wheels(&mut self, body: RigidBodyHandle) {
        self.wheel_groups
            .retain(|_, g| g.controller.chassis != body);
    }
}
fn wheel_pose(body: &RigidBody, wheel: &Wheel, spec: &WheelSpec3d) -> ([f32; 3], [f32; 4], f32) {
    let length = if wheel.raycast_info().is_in_contact {
        wheel.raycast_info().suspension_length
    } else {
        spec.rest
    };
    let position =
        body.position() * Vector::from_array(spec.at) - body.rotation() * Vector::Y * length;
    let rotation = (glam::Quat::from_array(body.rotation().to_array())
        * glam::Quat::from_rotation_y(wheel.steering)
        * glam::Quat::from_rotation_x(wheel.rotation))
    .normalize();
    (position.to_array(), rotation.to_array(), length)
}
fn wheel_collider_pose(spec: &WheelSpec3d, length: f32, steering: f32) -> Pose {
    let rotation = glam::Quat::from_rotation_y(steering)
        * glam::Quat::from_rotation_z(std::f32::consts::FRAC_PI_2);
    Pose::from_parts(
        Vector::from_array(spec.at) - Vector::Y * length,
        Rotation::from_array(rotation.to_array()),
    )
}
