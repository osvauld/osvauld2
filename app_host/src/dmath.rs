//! Same math on every device (`docs/design/jetpack-mp.md` §4): Luau's transcendental `math`
//! functions call the platform's libm, which differs by an ulp between glibc, Apple and bionic —
//! enough to desync a lockstep match. They are replaced, for every app, by the `libm` crate's
//! software versions. `+ - * /`, `sqrt`, `floor`, `fmod` are IEEE-exact and stay native.
//!
//! The replacement alone is not enough: the compiler turns `math.sin(x)` into a direct C call
//! that never looks in the table. Marking `math` mutable turns that fast path off for all of
//! `math` (floor and min included); the cost is a normal call instead of a fast one. It must be
//! set before the first chunk loads — a compiler setting is not retroactive.
//!
//! Outside the promise, said in the guide: `^` (folded at compile time and run at run time with
//! the platform's `pow`), and `string.format` of floats (the platform's `snprintf`).

use mlua::{Compiler, Error, Lua, Table};

/// Whether this VM's float `%` is unfused, as `.cargo/config.toml` asks of the C++ build. A
/// stale or overridden build fails it; anything that needs devices to agree refuses then.
#[allow(dead_code)] // read by `net.match` (step 5)
pub(crate) struct Agrees(pub bool);

pub(crate) fn install(vm: &Lua) -> mlua::Result<()> {
    vm.set_compiler(Compiler::new().set_mutable_globals(vec!["math".into(), "vector".into()]));
    let math: Table = vm.globals().get("math")?;
    let one = |name: &str, f: fn(f64) -> f64| -> mlua::Result<()> {
        math.set(name, vm.create_function(move |_, x: f64| Ok(f(x)))?)
    };
    one("sin", libm::sin)?;
    one("cos", libm::cos)?;
    one("tan", libm::tan)?;
    one("asin", libm::asin)?;
    one("acos", libm::acos)?;
    one("atan", libm::atan)?;
    one("sinh", libm::sinh)?;
    one("cosh", libm::cosh)?;
    one("tanh", libm::tanh)?;
    one("exp", libm::exp)?;
    one("log10", libm::log10)?;
    math.set("atan2", vm.create_function(|_, (y, x): (f64, f64)| Ok(libm::atan2(y, x)))?)?;
    math.set("pow", vm.create_function(|_, (x, y): (f64, f64)| Ok(libm::pow(x, y)))?)?;
    // Luau's own special cases, kept so a base-2 or base-10 log reads the same as before.
    math.set(
        "log",
        vm.create_function(|_, (x, base): (f64, Option<f64>)| {
            Ok(match base {
                None => libm::log(x),
                Some(2.0) => libm::log2(x),
                Some(10.0) => libm::log10(x),
                Some(b) => libm::log(x) / libm::log(b),
            })
        })?,
    )?;
    // Plain arithmetic, but in C++ that a compiler may fuse into one rounding on arm64 and not
    // on x86_64; Rust never fuses.
    math.set(
        "lerp",
        vm.create_function(|_, (a, b, t): (f64, f64, f64)| Ok(if t == 1.0 { b } else { a + (b - a) * t }))?,
    )?;
    math.set(
        "map",
        vm.create_function(|_, (x, in0, in1, out0, out1): (f64, f64, f64, f64, f64)| {
            Ok(out0 + (x - in0) * (out1 - out0) / (in1 - in0))
        })?,
    )?;
    math.set("noise", refused(vm, "math.noise is not the same on every device; build noise from math.random with a seed")?)?;
    // float32 with a native `atan2` in `vector.angle`; no app uses it, so it goes rather than
    // getting a second set of replacements. Every name is set: the compiler resolves
    // `vector.create` by plain lookup, so a missing key would read as nil, not as this error.
    let vector = vm.create_table()?;
    let no = refused(vm, "vector is not the same on every device; use numbers")?;
    for name in [
        "create", "magnitude", "normalize", "cross", "dot", "angle", "floor", "ceil", "abs", "sign",
        "clamp", "max", "min", "zero", "one",
    ] {
        vector.set(name, no.clone())?;
    }
    vm.globals().set("vector", vector)?;
    let agrees = probe(vm)?;
    vm.set_app_data(Agrees(agrees));
    Ok(())
}

fn refused(vm: &Lua, why: &'static str) -> mlua::Result<mlua::Function> {
    vm.create_function(move |_, _: mlua::MultiValue| -> mlua::Result<()> { Err(Error::runtime(why)) })
}

// An input where one rounding and two differ (checked by `float_mod_is_not_fused`).
fn probe(vm: &Lua) -> mlua::Result<bool> {
    let (a, b): (f64, f64) = (3.0 * 1.37 + 0.1, 0.7 + 3.0 * 0.013);
    let got: f64 = vm.load("local a, b = ... return a % b").call((a, b))?;
    Ok(got.to_bits() == (a - (a / b).floor() * b).to_bits())
}
