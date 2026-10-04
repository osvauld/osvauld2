use super::*;
use crate::gfx::{camera3d, named_fields, quat, vec3};

pub(super) struct WorldHandle {
    pub(super) id: String,
    pub(super) scene: Arc<Scene3d>,
    pub(super) host: Host,
}

impl UserData for WorldHandle {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method(
            "scene",
            |lua, this, (camera, options): (Option<Table>, Option<Table>)| {
                let running = if let Some(options) = options {
                    named_fields(&options, "world3d:scene options", &["running"])?;
                    match options.get::<mlua::Value>("running")? {
                        mlua::Value::Nil => true,
                        mlua::Value::Boolean(value) => value,
                        _ => return Err(Error::runtime("world3d:scene running must be a boolean")),
                    }
                } else {
                    true
                };
                let scene = match camera {
                    Some(camera) => Scene3d::new(camera3d(&camera)?, this.scene.objects.to_vec())
                        .map_err(Error::external)?,
                    None => this.scene.clone(),
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
