//! `sound`: generated sounds and one-shots (docs/design/sound.md). The host hands the app a
//! [`SoundOut`] — a channel to the shell's mixer and this app's bus. Without one (headless tests,
//! a host with no mixer) every call still checks its arguments and simply plays nothing, so an
//! app behaves the same everywhere apart from the speaker.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::mpsc::Sender;

use audio::{Command, Sound, Synth, Wave};
use mlua::{Error, Lua, Table, UserData, UserDataRef, Value};

/// Where an app's sounds go: the mixer's command channel and the app's bus on it.
#[derive(Clone)]
pub struct SoundOut {
    pub tx: Sender<Command>,
    pub bus: u64,
}

/// Outlives the VM like the doc cores: a reload keeps the same bus, and voice ids keep counting
/// so a new VM's one-shot never replaces one the old VM started.
#[derive(Default)]
pub(crate) struct State {
    out: Option<SoundOut>,
    next: u64,
}

pub(crate) type Hook = Rc<RefCell<State>>;

pub(crate) fn connect(hook: &Hook, out: SoundOut) {
    hook.borrow_mut().out = Some(out);
}

#[derive(Clone)]
pub(crate) struct LuaSound(pub Sound);

impl UserData for LuaSound {}

const SYNTH_FIELDS: &[&str] =
    &["name", "wave", "freq", "slide", "attack", "sustain", "decay", "volume", "duty", "loop"];

pub(crate) fn install(lua: &Lua, hook: Hook, viewing: Rc<Cell<bool>>) -> mlua::Result<()> {
    let sound = lua.create_table()?;
    sound.set("synth", lua.create_function(|lua, spec: Table| lua.create_userdata(LuaSound(synth(&spec)?)))?)?;
    sound.set(
        "play",
        lua.create_function(move |_, (s, options): (Value, Option<Table>)| {
            if viewing.get() {
                return Err(Error::runtime(
                    "sound.play: not while view() describes — play from a handler (on_click, on_frame, on_zone…)",
                ));
            }
            let Value::UserData(ud) = &s else {
                return Err(Error::runtime("sound.play: give it a sound from sound.synth"));
            };
            let s: UserDataRef<LuaSound> =
                ud.borrow().map_err(|_| Error::runtime("sound.play: give it a sound from sound.synth"))?;
            if s.0.looping() {
                return Err(Error::runtime(format!(
                    "sound.play: {} loops, so it would never stop — describe it with ui.sound instead",
                    s.0.name()
                )));
            }
            let (volume, pitch) = play_options(options.as_ref())?;
            let mut state = hook.borrow_mut();
            state.next += 1;
            let voice = state.next;
            if let Some(out) = &state.out {
                let _ = out.tx.send(Command::Play { bus: out.bus, voice, sound: s.0.clone(), volume, pitch });
            }
            Ok(())
        })?,
    )?;
    sound.set_readonly(true);
    lua.globals().set("sound", sound)
}

fn play_options(options: Option<&Table>) -> mlua::Result<(f32, f32)> {
    let Some(o) = options else { return Ok((1.0, 1.0)) };
    fields(o, "sound.play", &["volume", "pitch"])?;
    let volume = number(o, "sound.play", "volume")?.unwrap_or(1.0);
    let pitch = number(o, "sound.play", "pitch")?.unwrap_or(1.0);
    if !(0.0..=1.0).contains(&volume) {
        return Err(Error::runtime(format!("sound.play: volume must be 0 to 1, got {volume}")));
    }
    if !(pitch > 0.0 && pitch <= 8.0) {
        return Err(Error::runtime(format!("sound.play: pitch must be above 0 and at most 8, got {pitch}")));
    }
    Ok((volume, pitch))
}

pub(crate) fn synth(spec: &Table) -> mlua::Result<Sound> {
    const AT: &str = "sound.synth";
    fields(spec, AT, SYNTH_FIELDS)?;
    let d = Synth::default();
    let wave = match spec.get::<Value>("wave")? {
        Value::Nil => d.wave,
        Value::String(w) => match &*w.to_str()? {
            "square" => Wave::Square,
            "saw" => Wave::Saw,
            "sine" => Wave::Sine,
            "noise" => Wave::Noise,
            other => {
                return Err(Error::runtime(format!(
                    "{AT}: wave must be square, saw, sine or noise, got {other}"
                )));
            }
        },
        _ => return Err(Error::runtime(format!("{AT}: wave must be a string"))),
    };
    let looping = match spec.get::<Value>("loop")? {
        Value::Nil => false,
        Value::Boolean(b) => b,
        _ => return Err(Error::runtime(format!("{AT}: loop must be true or false"))),
    };
    let name = match spec.get::<Value>("name")? {
        Value::Nil => "synth".to_string(),
        Value::String(s) => s.to_str()?.to_string(),
        _ => return Err(Error::runtime(format!("{AT}: name must be a string"))),
    };
    let s = Synth {
        wave,
        freq: number(spec, AT, "freq")?.unwrap_or(d.freq),
        slide: number(spec, AT, "slide")?.unwrap_or(d.slide),
        attack: number(spec, AT, "attack")?.unwrap_or(d.attack),
        sustain: number(spec, AT, "sustain")?.unwrap_or(d.sustain),
        decay: number(spec, AT, "decay")?.unwrap_or(d.decay),
        volume: number(spec, AT, "volume")?.unwrap_or(d.volume),
        duty: number(spec, AT, "duty")?.unwrap_or(d.duty),
        looping,
    };
    Sound::synth(&name, s).map_err(|e| Error::runtime(format!("{AT} {name}: {e}")))
}

fn fields(table: &Table, owner: &str, allowed: &[&str]) -> mlua::Result<()> {
    for pair in table.clone().pairs::<Value, Value>() {
        let (key, _) = pair?;
        let ok = match &key {
            Value::String(k) => allowed.contains(&&*k.to_str()?),
            _ => false,
        };
        if !ok {
            let key = match key {
                Value::String(k) => k.to_str()?.to_string(),
                other => format!("{other:?}"),
            };
            return Err(Error::runtime(format!(
                "{owner}: unknown field {key} (it takes {})",
                allowed.join(", ")
            )));
        }
    }
    Ok(())
}

fn number(table: &Table, owner: &str, field: &str) -> mlua::Result<Option<f32>> {
    match table.get::<Value>(field)? {
        Value::Nil => Ok(None),
        Value::Integer(i) => Ok(Some(i as f32)),
        Value::Number(n) => Ok(Some(n as f32)),
        _ => Err(Error::runtime(format!("{owner}: {field} must be a number"))),
    }
}
