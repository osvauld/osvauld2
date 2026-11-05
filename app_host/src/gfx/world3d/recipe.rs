use super::*;
use crate::gfx::{LuaScene3d, named_fields, need, need_gfx, positional_len, quat, vec3};
use world::world3d::{EntitySpec3d, Shape3d, WorldRecipe3d};

pub(super) fn parse(spec: Table) -> mlua::Result<(String, Arc<Scene3d>, WorldRecipe3d)> {
    named_fields(
        &spec,
        "world3d",
        &["id", "scene", "bodies", "gravity", "wheels"],
    )?;
    let mut recipe = WorldRecipe3d::default();
    if let Some(gravity) = spec.get::<Option<Table>>("gravity")? {
        for index in 1..=3 {
            if !matches!(
                gravity.get::<mlua::Value>(index)?,
                mlua::Value::Integer(_) | mlua::Value::Number(_)
            ) {
                return Err(Error::runtime("world3d.gravity must contain numbers"));
            }
        }
        recipe.gravity = vec3(gravity, "world3d.gravity")?.to_array();
    }
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
                "id",
                "position",
                "rotation",
                "dynamic",
                "sphere",
                "box",
                "mesh",
                "sensor",
                "mass",
                "center",
                "friction",
                "restitution",
            ],
        )?;
        let sphere: Option<f32> = body.get("sphere")?;
        let box_size: Option<Table> = body.get("box")?;
        let has_mesh = body.contains_key("mesh")?;
        let shape = match (sphere, box_size, has_mesh) {
            (Some(radius), None, false) => Shape3d::Sphere(radius),
            (None, Some(size), false) => Shape3d::Box(vec3(size, "body.box")?.to_array()),
            (None, None, true) => Shape3d::Mesh(need_gfx::<crate::gfx::mesh::LuaMesh, _>(
                &body,
                "world3d body",
                "mesh",
                "a gfx.mesh",
                |m| m.0.clone(),
            )?),
            _ => {
                return Err(Error::runtime(
                    "world3d body needs exactly one of sphere, box or mesh",
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
        let properties = world::world3d::BodyProperties3d {
            mass: if body.contains_key("mass")? {
                Some(wheel::number(&body, "mass", None)?)
            } else {
                None
            },
            center: body
                .get::<Option<Table>>("center")?
                .map(|v| wheel::vector(v, "center"))
                .transpose()?
                .unwrap_or([0.0; 3]),
            friction: wheel::number(&body, "friction", Some(0.5))?,
            restitution: wheel::number(&body, "restitution", Some(0.35))?,
        };
        recipe.properties.insert(body_id.clone(), properties);
        recipes.push(EntitySpec3d {
            id: body_id,
            shape,
            position: wheel::vector(need(&body, "world3d body", "position")?, "body.position")?,
            rotation: body
                .get::<Option<Table>>("rotation")?
                .map(|r| {
                    for i in 1..=4 {
                        if !matches!(
                            r.get::<mlua::Value>(i)?,
                            mlua::Value::Integer(_) | mlua::Value::Number(_)
                        ) {
                            return Err(Error::runtime("body rotation must contain numbers"));
                        }
                    }
                    quat(r, "body.rotation").map(|q| q.to_array())
                })
                .transpose()?
                .unwrap_or(glam::Quat::IDENTITY.to_array()),
            dynamic: flag(&body, "dynamic")?,
            sensor: flag(&body, "sensor")?,
        });
    }
    recipe.bodies = recipes;
    if let Some(wheels) = spec.get::<Option<Table>>("wheels")? {
        recipe.wheels = wheel::parse(wheels)?;
        for object in scene
            .objects
            .iter()
            .filter(|o| recipe.wheels.iter().any(|w| w.id == o.id.as_ref()))
        {
            if object.rotation != glam::Quat::IDENTITY && object.rotation != -glam::Quat::IDENTITY {
                return Err(Error::runtime(
                    "bound wheel visuals require identity authored rotation",
                ));
            }
        }
    }
    Ok((id, scene, recipe))
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
