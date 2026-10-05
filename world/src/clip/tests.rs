use super::*;

fn key(time: f64, value: f64) -> Key {
    Key {
        time,
        value,
        easing: Easing::Linear,
    }
}

fn track(part: &str, prop: Prop, keys: Vec<Key>) -> Track {
    Track {
        part: part.into(),
        prop,
        keys,
    }
}

fn swing(looped: bool) -> Clip {
    let keys = vec![key(0.0, 0.0), key(0.5, 90.0), key(1.0, 0.0)];
    Clip::new(1.0, looped, vec![track("lid", Prop::Rot, keys)]).unwrap()
}

#[test]
fn a_track_interpolates_between_keys_and_holds_outside_them() {
    let clip = Clip::new(
        2.0,
        false,
        vec![track("arm", Prop::X, vec![key(0.5, 10.0), key(1.5, 30.0)])],
    )
    .unwrap();
    assert_eq!(clip.sample(0.0)["arm"].x, 10.0, "before the first key");
    assert_eq!(clip.sample(1.0)["arm"].x, 20.0);
    assert_eq!(clip.sample(1.8)["arm"].x, 30.0, "after the last key");
}

#[test]
fn untouched_properties_stay_at_rest() {
    let pose = swing(false).sample(0.25)["lid"];
    assert_eq!(
        pose,
        Pose {
            rot: 45.0,
            ..Pose::default()
        }
    );
}

#[test]
fn a_looped_clip_wraps_and_a_once_clip_holds_its_end() {
    assert_eq!(swing(true).sample(1.25)["lid"].rot, 45.0);
    assert_eq!(swing(true).sample(-0.25)["lid"].rot, 45.0);
    let once = Clip::new(
        1.0,
        false,
        vec![track("lid", Prop::Rot, vec![key(0.0, 0.0), key(1.0, 90.0)])],
    )
    .unwrap();
    assert_eq!(once.sample(5.0)["lid"].rot, 90.0);
}

#[test]
fn a_keys_easing_shapes_the_segment_arriving_at_it() {
    let eased = Key {
        easing: Easing::InOut,
        ..key(1.0, 100.0)
    };
    let clip = Clip::new(
        1.0,
        false,
        vec![track("lid", Prop::Y, vec![key(0.0, 0.0), eased])],
    )
    .unwrap();
    let (early, mid) = (clip.sample(0.25)["lid"].y, clip.sample(0.5)["lid"].y);
    assert!(early < 25.0, "slow start, got {early}");
    assert!((mid - 50.0).abs() < 1e-9);
}

#[test]
fn a_bad_clip_is_refused() {
    let ok = || vec![key(0.0, 0.0), key(1.0, 1.0)];
    let err = |length: f64, tracks: Vec<Track>| Clip::new(length, true, tracks).unwrap_err();
    assert_eq!(err(0.0, vec![]), ClipError::Length);
    assert_eq!(err(f64::NAN, vec![]), ClipError::Length);
    let (part, prop) = ("lid".to_string(), Prop::Rot);
    assert_eq!(
        err(1.0, vec![track("lid", prop, vec![])]),
        ClipError::NoKeys {
            part: part.clone(),
            prop
        }
    );
    let late = vec![key(0.0, 0.0), key(2.0, 1.0)];
    assert_eq!(
        err(1.0, vec![track("lid", prop, late)]),
        ClipError::KeyTime {
            part: part.clone(),
            prop
        }
    );
    let backwards = vec![key(0.5, 0.0), key(0.5, 1.0)];
    assert_eq!(
        err(1.0, vec![track("lid", prop, backwards)]),
        ClipError::KeyTime {
            part: part.clone(),
            prop
        }
    );
    let twice = vec![track("lid", prop, ok()), track("lid", prop, ok())];
    assert_eq!(
        err(1.0, twice),
        ClipError::DuplicateTrack {
            part: part.clone(),
            prop
        }
    );
    let inf = vec![key(0.0, f64::INFINITY)];
    assert_eq!(
        err(1.0, vec![track("lid", prop, inf)]),
        ClipError::Value { part, prop }
    );
}
