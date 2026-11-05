use super::*;
use crate::gfx::{named_fields, need, positional_len, vec3};
use world::world3d::{WheelInput3d, WheelMotor3d, WheelSpec3d};

pub(super) fn number(t: &Table, name: &str, default: Option<f32>) -> mlua::Result<f32> {
    match t.get::<mlua::Value>(name)? {
        mlua::Value::Nil => default.ok_or_else(|| Error::runtime(format!("physics needs {name}"))),
        mlua::Value::Integer(v) => Ok(v as f32),
        mlua::Value::Number(v) => Ok(v as f32),
        _ => Err(Error::runtime(format!("physics {name} must be a number"))),
    }
}
pub(super) fn vector(t: Table, name: &str) -> mlua::Result<[f32; 3]> {
    for i in 1..=3 {
        if !matches!(
            t.get::<mlua::Value>(i)?,
            mlua::Value::Integer(_) | mlua::Value::Number(_)
        ) {
            return Err(Error::runtime(format!(
                "physics {name} must contain numbers"
            )));
        }
    }
    vec3(t, name).map(|v| v.to_array())
}
pub(super) fn parse(list: Table) -> mlua::Result<Vec<WheelSpec3d>> {
    let n = positional_len(&list, "world3d.wheels")?;
    if n > 128 {
        return Err(Error::runtime("at most 128 wheels per world"));
    }
    let mut wheels = Vec::with_capacity(n);
    for i in 1..=n {
        let w: Table = list.get(i)?;
        named_fields(
            &w,
            "wheel",
            &[
                "id",
                "body",
                "at",
                "radius",
                "width",
                "rest",
                "travel",
                "stiffness",
                "damping",
                "grip",
            ],
        )?;
        wheels.push(WheelSpec3d {
            id: need(&w, "wheel", "id")?,
            body: need(&w, "wheel", "body")?,
            at: vector(need(&w, "wheel", "at")?, "at")?,
            radius: number(&w, "radius", None)?,
            width: number(&w, "width", None)?,
            rest: number(&w, "rest", None)?,
            travel: number(&w, "travel", None)?,
            stiffness: number(&w, "stiffness", None)?,
            damping: number(&w, "damping", None)?,
            grip: number(&w, "grip", None)?,
        });
    }
    Ok(wheels)
}
pub(super) fn commands(list: Table) -> mlua::Result<Vec<(String, WheelInput3d)>> {
    let n = positional_len(&list, "world3d:set_wheels")?;
    if n > 128 {
        return Err(Error::runtime("at most 128 wheel commands"));
    }
    let mut commands = Vec::with_capacity(n);
    for i in 1..=n {
        let t: Table = list.get(i)?;
        named_fields(
            &t,
            "wheel command",
            &["id", "torque", "brake", "steering", "motor"],
        )?;
        let motor = t
            .get::<Option<Table>>("motor")?
            .map(|m| {
                named_fields(&m, "wheel motor", &["velocity", "max_torque"])?;
                Ok::<_, Error>(WheelMotor3d {
                    velocity: number(&m, "velocity", None)?,
                    max_torque: number(&m, "max_torque", None)?,
                })
            })
            .transpose()?;
        commands.push((
            need(&t, "wheel command", "id")?,
            WheelInput3d {
                torque: number(&t, "torque", Some(0.0))?,
                brake: number(&t, "brake", Some(0.0))?,
                steering: number(&t, "steering", Some(0.0))?,
                motor,
            },
        ));
    }
    Ok(commands)
}
