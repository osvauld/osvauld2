use super::*;
use crate::gfx::{camera3d, named_fields, quat, vec3};

pub(super) struct WorldHandle {
    pub(super) id: String,
    pub(super) scene: Arc<Scene3d>,
    pub(super) host: Host,
}

impl UserData for WorldHandle {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("get", |lua, this, id: String| {
            let worlds = this.host.worlds.borrow();
            let Some(world) = worlds.get(&this.id) else {
                return Ok(None);
            };
            let Ok(body) = world.body(&id) else {
                // A wheel reports its live pose only: it has no velocity of its own.
                let Ok((position, rotation)) = world.pose(&id) else {
                    return Ok(None);
                };
                let out = lua.create_table()?;
                out.set("position", lua.create_sequence_from(position.to_array())?)?;
                out.set("rotation", lua.create_sequence_from(rotation.to_array())?)?;
                out.set("tick", world.tick())?;
                return Ok(Some(out));
            };
            let out = lua.create_table()?;
            out.set("position", lua.create_sequence_from(body.position)?)?;
            out.set("rotation", lua.create_sequence_from(body.rotation)?)?;
            out.set("velocity", lua.create_sequence_from(body.velocity)?)?;
            out.set(
                "angular_velocity",
                lua.create_sequence_from(body.angular_velocity)?,
            )?;
            out.set("sleeping", body.sleeping)?;
            out.set("tick", world.tick())?;
            Ok(Some(out))
        });
        methods.add_method("set_wheels", |_, this, spec: Table| {
            if !this.host.ready.get()
                || !(this.host.commanding.get() || this.host.control_input.get())
            {
                return Err(Error::runtime(
                    "world3d:set_wheels is only allowed in input or zone handlers",
                ));
            }
            let commands = wheel::commands(spec)?;
            this.host
                .worlds
                .borrow_mut()
                .get_mut(&this.id)
                .ok_or_else(|| Error::runtime("3D world is not active"))?
                .set_wheels(&commands)
                .map_err(Error::external)
        });
        methods.add_method(
            "scene",
            |lua, this, (camera, options): (Option<Table>, Option<Table>)| {
                let (running, poses) = if let Some(options) = options {
                    named_fields(&options, "world3d:scene options", &["running", "poses"])?;
                    let running = match options.get::<mlua::Value>("running")? {
                        mlua::Value::Nil => true,
                        mlua::Value::Boolean(value) => value,
                        _ => return Err(Error::runtime("world3d:scene running must be a boolean")),
                    };
                    (running, options.get::<Option<Table>>("poses")?)
                } else {
                    (true, None)
                };
                let scene = match (camera, poses) {
                    (None, None) => this.scene.clone(),
                    (camera, poses) => {
                        let mut objects = this.scene.objects.to_vec();
                        if let Some(poses) = poses {
                            override_poses(&mut objects, poses)?;
                        }
                        let camera = match camera {
                            Some(camera) => camera3d(&camera)?,
                            None => this.scene.camera.clone(),
                        };
                        Scene3d::new(camera, objects).map_err(Error::external)?
                    }
                };
                lua.create_userdata(SceneHandle {
                    id: this.id.clone(),
                    scene,
                    host: this.host.clone(),
                    running,
                })
            },
        );
        methods.add_method("set", |_, this, (id, spec): (String, Table)| {
            if !this.host.ready.get() || !this.host.commanding.get() {
                return Err(Error::runtime(
                    "world3d:set is only allowed in input handlers or on_zone",
                ));
            }
            named_fields(
                &spec,
                "world3d:set",
                &["pos", "rotation", "velocity", "spin"],
            )?;
            let vector = |name| -> mlua::Result<Option<[f32; 3]>> {
                spec.get::<Option<Table>>(name)?
                    .map(|v| {
                        numbers(&v, name, 3)?;
                        vec3(v, name).map(|v| v.to_array())
                    })
                    .transpose()
            };
            let to = world::world3d::Set3d {
                pos: vector("pos")?,
                velocity: vector("velocity")?,
                spin: vector("spin")?,
                rotation: spec
                    .get::<Option<Table>>("rotation")?
                    .map(|v| {
                        numbers(&v, "rotation", 4)?;
                        quat(v, "set.rotation").map(|q| q.to_array())
                    })
                    .transpose()?,
            };
            this.host
                .worlds
                .borrow_mut()
                .get_mut(&this.id)
                .ok_or_else(|| Error::runtime("3D world is not active"))?
                .set(&id, to)
                .map_err(Error::external)
        });
        methods.add_method("reset", |_, this, id: String| {
            if !this.host.ready.get() || !this.host.commanding.get() {
                return Err(Error::runtime(
                    "world3d:reset is only allowed in input handlers or on_zone",
                ));
            }
            let mut worlds = this.host.worlds.borrow_mut();
            worlds
                .get_mut(&this.id)
                .ok_or_else(|| Error::runtime("3D world is not active"))?
                .reset(&id)
                .map_err(Error::external)
        });
    }
}

/// This frame's authored pose for named visuals, before the world composes them: in their
/// host's frame when attached. What moves and why is the app's; the authored scene is untouched.
fn override_poses(objects: &mut [runtime::scene3d::Object3d], poses: Table) -> mlua::Result<()> {
    for pair in poses.pairs::<String, Table>() {
        let (id, pose) = pair?;
        let object = objects
            .iter_mut()
            .find(|o| o.id.as_ref() == id)
            .ok_or_else(|| Error::runtime(format!("world3d:scene poses: no visual {id}")))?;
        named_fields(&pose, "world3d:scene pose", &["position", "rotation"])?;
        if let Some(v) = pose.get::<Option<Table>>("position")? {
            object.position = vec3(v, "pose.position")?;
        }
        if let Some(v) = pose.get::<Option<Table>>("rotation")? {
            object.rotation = quat(v, "pose.rotation")?;
        }
    }
    Ok(())
}

fn numbers(table: &Table, name: &str, size: usize) -> mlua::Result<()> {
    for index in 1..=size {
        if !matches!(
            table.get::<mlua::Value>(index)?,
            mlua::Value::Integer(_) | mlua::Value::Number(_)
        ) {
            return Err(Error::runtime(format!(
                "world3d:set {name} must contain numbers"
            )));
        }
    }
    Ok(())
}
