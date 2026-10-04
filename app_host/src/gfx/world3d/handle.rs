use super::*;
use crate::gfx::camera3d;

pub(super) struct WorldHandle {
    pub(super) id: String,
    pub(super) scene: Arc<Scene3d>,
    pub(super) host: Host,
}

impl UserData for WorldHandle {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("scene", |lua, this, camera: Option<Table>| {
            let scene = match camera {
                Some(camera) => Scene3d::new(camera3d(&camera)?, this.scene.objects.to_vec())
                    .map_err(Error::external)?,
                None => this.scene.clone(),
            };
            lua.create_userdata(SceneHandle {
                id: this.id.clone(),
                scene,
                host: this.host.clone(),
            })
        });
        methods.add_method("reset", |_, this, id: String| {
            if !this.host.ready.get() || !this.host.commanding.get() {
                return Err(Error::runtime(
                    "world3d:reset is only allowed in input handlers",
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
