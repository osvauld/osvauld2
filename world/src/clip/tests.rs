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

fn eased(time: f64, value: f64, easing: Easing) -> Key {
    Key { easing, ..key(time, value) }
}

#[test]
fn scale_x_and_scale_y_are_tracks_of_their_own() {
    let clip = Clip::new(
        1.0,
        false,
        vec![
            track("arm", Prop::ScaleX, vec![key(0.0, 1.0), key(1.0, 3.0)]),
            track("arm", Prop::ScaleY, vec![key(0.0, 1.0), key(1.0, 0.5)]),
        ],
    )
    .unwrap();
    let pose = clip.sample(0.5)["arm"];
    assert_eq!((pose.scale_x, pose.scale_y, pose.scale), (2.0, 0.75, 1.0));
}

// U-C2
#[test]
fn a_step_key_holds_the_previous_value_until_its_own_time() {
    for prop in [Prop::X, Prop::Y, Prop::Rot, Prop::Scale, Prop::ScaleX, Prop::ScaleY] {
        let keys = vec![key(0.0, 1.0), eased(0.5, 4.0, Easing::Step), key(1.0, 6.0)];
        let clip = Clip::new(1.0, false, vec![track("arm", prop, keys)]).unwrap();
        let at = |t: f64| {
            let p = clip.sample(t)["arm"];
            match prop {
                Prop::X => p.x,
                Prop::Y => p.y,
                Prop::Rot => p.rot,
                Prop::Scale => p.scale,
                Prop::ScaleX => p.scale_x,
                Prop::ScaleY => p.scale_y,
            }
        };
        assert_eq!(at(0.0), 1.0, "{prop:?}");
        assert_eq!(at(0.4999), 1.0, "{prop:?} holds before the step key");
        assert_eq!(at(0.5), 4.0, "{prop:?} jumps at the key's own time");
        assert_eq!(at(0.75), 5.0, "{prop:?}: the next key's linear segment is untouched");
    }
}

// U-C3: independently computed (60-digit bisection, the scratch `bezier_ref.py`).
const CSS_TABLE: [((f64, f64, f64, f64), [f64; 5]); 6] = [
    ((0.25, 0.1, 0.25, 1.0), [0.094796305716, 0.408510591355, 0.802403387585, 0.960458978349, 0.994316477485]),
    ((0.42, 0.0, 1.0, 1.0), [0.0170266096516, 0.0934646507188, 0.315356812573, 0.621861869175, 0.839427845762]),
    ((0.0, 0.0, 0.58, 1.0), [0.160572154238, 0.378138130825, 0.684643187427, 0.906535349281, 0.982973390348]),
    ((0.42, 0.0, 0.58, 1.0), [0.0197224535483, 0.129161931047, 0.5, 0.870838068953, 0.980277546452]),
    ((0.68, -0.6, 0.32, 1.6), [-0.0728231807972, -0.0977077421231, 0.5, 1.09770774212, 1.0728231808]),
    ((0.1, 0.9, 0.2, 0.3), [0.465803147816, 0.579403110759, 0.66594662427, 0.805340950179, 0.916186413263]),
];

#[test]
fn a_bezier_key_is_css_cubic_bezier() {
    for ((x1, y1, x2, y2), want) in CSS_TABLE {
        let curve = Easing::bezier(x1, y1, x2, y2).unwrap();
        let keys = vec![key(0.0, 0.0), eased(1.0, 1.0, curve)];
        let clip = Clip::new(1.0, false, vec![track("arm", Prop::X, keys)]).unwrap();
        for (u, want) in [0.1, 0.25, 0.5, 0.75, 0.9].into_iter().zip(want) {
            let got = clip.sample(u)["arm"].x;
            assert!((got - want).abs() < 1e-6, "{x1},{y1},{x2},{y2} at {u}: {got} vs {want}");
        }
        assert_eq!(clip.sample(0.0)["arm"].x, 0.0);
        assert_eq!(clip.sample(1.0)["arm"].x, 1.0);
    }
}

#[test]
fn a_bezier_with_x_outside_0_to_1_is_refused() {
    for (x1, x2) in [(-0.1, 0.5), (0.5, 1.1), (f64::NAN, 0.5)] {
        assert_eq!(Easing::bezier(x1, 0.0, x2, 1.0), Err(ClipError::Bezier));
    }
    assert_eq!(Easing::bezier(0.5, f64::INFINITY, 0.5, 1.0), Err(ClipError::Bezier));
    // Built by hand past `bezier`, the clip still refuses it.
    let bad = Easing::Bezier { x1: 2.0, y1: 0.0, x2: 0.5, y2: 1.0 };
    let keys = vec![key(0.0, 0.0), eased(1.0, 1.0, bad)];
    let (part, prop) = ("lid".to_string(), Prop::Y);
    assert_eq!(
        Clip::new(1.0, false, vec![track("lid", prop, keys)]).unwrap_err(),
        ClipError::KeyEasing { part, prop }
    );
}

/// Only `+ − × ÷` and comparisons, a fixed count: these bits are what every machine computes.
/// They were taken from the same algorithm in Python (IEEE doubles, no fused multiply-add).
#[test]
fn the_bezier_solve_is_pinned_to_the_bit() {
    let curve = Easing::bezier(0.68, -0.6, 0.32, 1.6).unwrap();
    let bits: Vec<u64> = [0.1, 0.3, 0.7].iter().map(|&u| curve.apply(u).to_bits()).collect();
    assert_eq!(bits, PINNED_BITS, "{bits:#x?}");
}

const PINNED_BITS: [u64; 3] = [0xbfb2a48a3bea2dd8, 0xbfb193ed53e7b334, 0x3ff1193ed53e7b34];

fn shown(part: &str, keys: &[(f64, bool)]) -> ShowTrack {
    let keys = keys.iter().map(|&(time, show)| ShowKey { time, show }).collect();
    ShowTrack { part: part.into(), keys }
}

// U-C4 (the clip half): a `show` track is boolean and always stepped.
#[test]
fn a_show_track_is_stepped_and_holds_its_first_value_before_the_first_key() {
    let clip = Clip::new(1.0, false, vec![track("arm", Prop::X, vec![key(0.0, 1.0), key(1.0, 2.0)])])
        .unwrap()
        .with_shows(vec![shown("sword", &[(0.2, false), (0.4, true)])])
        .unwrap();
    let at = |t: f64| clip.sample(t)["sword"].show;
    assert!(!at(0.0), "before the first key: the first key's value");
    assert!(!at(0.2));
    assert!(!at(0.3999));
    assert!(at(0.4), "a key takes hold at its own time");
    assert!(at(1.0));
    assert!(clip.sample(0.5)["arm"].show, "a part with no show track stays shown");
    assert_eq!(clip.parts().filter(|p| *p == "sword").count(), 1, "the clip moves the part it shows");
}

#[test]
fn a_looped_show_track_wraps() {
    let clip = Clip::new(1.0, true, vec![]).unwrap().with_shows(vec![shown("a", &[(0.0, true), (0.5, false)])]).unwrap();
    assert!(clip.sample(1.25)["a"].show);
    assert!(!clip.sample(1.75)["a"].show);
}

#[test]
fn bad_show_tracks_are_refused() {
    let with = |s: ShowTrack| Clip::new(1.0, false, vec![]).unwrap().with_shows(vec![s]).unwrap_err();
    assert_eq!(with(shown("a", &[])), ClipError::NoShowKeys { part: "a".into() });
    assert_eq!(with(shown("a", &[(0.5, true), (0.5, false)])), ClipError::ShowKeyTime { part: "a".into() });
    assert_eq!(with(shown("a", &[(1.5, true)])), ClipError::ShowKeyTime { part: "a".into() });
    assert_eq!(with(shown("a", &[(f64::NAN, true)])), ClipError::ShowKeyTime { part: "a".into() });
    let twice = Clip::new(1.0, false, vec![]).unwrap().with_shows(vec![shown("a", &[(0.0, true)]), shown("a", &[(0.0, false)])]);
    assert_eq!(twice.unwrap_err(), ClipError::DuplicateShow { part: "a".into() });
}
