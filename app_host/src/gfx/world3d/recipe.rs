use super::*;
use crate::gfx::{LuaScene3d, named_fields, need, need_gfx, positional_len, quat, vec3};
use world::world3d::{EntitySpec3d, Shape3d};

pub(super) fn parse(spec: Table) -> mlua::Result<(String, Arc<Scene3d>, Vec<EntitySpec3d>)> {
    named_fields(&spec, "world3d", &["id", "scene", "bodies"])?;
    let id: String = need(&spec, "world3d", "id")?;
    let scene =
        need_gfx::<LuaScene3d, _>(&spec, "world3d", "scene", "a gfx.scene3d", |s| s.0.clone())?;
    let bodies: Table = need(&spec, "world3d", "bodies")?;
    let len = positional_len(&bodies, "world3d.bodies")?;
    if len > 256 {
        return Err(Error::runtime("3D world is limited to 256 bodies"));
    }
    let mut recipes = Vec::with_capacity(len);
    for index in 1..=len {
        let body: Table = bodies.get(index)?;
        named_fields(
            &body,
            "world3d body",
            &[
                "id", "position", "rotation", "dynamic", "sphere", "box", "sensor",
            ],
        )?;
        let sphere: Option<f32> = body.get("sphere")?;
        let box_size: Option<Table> = body.get("box")?;
        let shape = match (sphere, box_size) {
            (Some(radius), None) => Shape3d::Sphere(radius),
            (None, Some(size)) => Shape3d::Box(vec3(size, "body.box")?.to_array()),
            _ => {
                return Err(Error::runtime(
                    "world3d body needs exactly one of sphere or box",
                ));
            }
        };
        let body_id: String = need(&body, "world3d body", "id")?;
        if let Some(object) = scene.objects.iter().find(|o| o.id.as_ref() == body_id) {
            if object.rotation != glam::Quat::IDENTITY && object.rotation != -glam::Quat::IDENTITY {
                return Err(Error::runtime(
                    "bound 3D body visuals require identity authored rotation",
                ));
            }
        }
        recipes.push(EntitySpec3d {
            id: body_id,
            shape,
            position: vec3(need(&body, "world3d body", "position")?, "body.position")?.to_array(),
            rotation: body
                .get::<Option<Table>>("rotation")?
                .map(|r| quat(r, "body.rotation").map(|q| q.to_array()))
                .transpose()?
                .unwrap_or(glam::Quat::IDENTITY.to_array()),
            dynamic: flag(&body, "dynamic")?,
            sensor: flag(&body, "sensor")?,
        });
    }
    Ok((id, scene, recipes))
}

fn flag(body: &Table, name: &str) -> mlua::Result<bool> {
    match body.get::<mlua::Value>(name)? {
        mlua::Value::Nil => Ok(false),
        mlua::Value::Boolean(value) => Ok(value),
        _ => Err(Error::runtime(format!(
            "world3d body {name} must be a boolean"
        ))),
    }
}
