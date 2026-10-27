use super::*;
use std::sync::mpsc::channel;

fn scope() -> Scope { Scope { instance: 1, revision: 1, size: Some((400.0, 300.0)) } }
fn snapshot() -> Snapshot { Snapshot { id: "app".into(), scope: scope(), rects: Arc::new([]) } }

#[test]
fn stale_accounts_closed_apps_resizes_and_reopened_instances_are_refused() {
    let snapshots = [snapshot()];
    assert!(validate(3, 3, &snapshots, |_| Some(scope())).is_ok());
    assert!(validate(3, 4, &snapshots, |_| Some(scope())).is_err());
    assert!(validate(3, 3, &snapshots, |_| None).is_err());
    assert!(validate(3, 3, &snapshots, |_| Some(Scope { instance: 2, ..scope() })).is_err());
    assert!(validate(3, 3, &snapshots, |_| Some(Scope { size: Some((300.0, 400.0)), ..scope() })).is_err());
}

#[test]
fn sampling_retains_the_identity_scope_and_actual_geometry() {
    let (tx, rx) = channel();
    let mut sample = snapshot();
    sample.rects = Arc::from([runtime::ElRect { id: "fire".into(), x: 20.0, y: 30.0, w: 50.0, h: 60.0,
        hits: vec!["touch"] }]);
    tx.send(Ok(sample)).unwrap();
    let mut snapshots = collect(vec![Request { id: "app".into(), scope: scope(), reply: rx }], Duration::ZERO).unwrap();
    let got = snapshots.pop().unwrap();
    assert_eq!(&*got.id, "app");
    assert_eq!(got.scope, scope());
    assert_eq!(got.rects[0].id, "fire");
    assert_eq!(got.rects[0].x, 20.0);
}

#[test]
fn a_missing_or_disconnected_app_cannot_hold_the_sampler_open() {
    let (tx, rx) = channel();
    assert!(collect(vec![Request { id: "slow".into(), scope: scope(), reply: rx }], Duration::ZERO).is_err());
    drop(tx);
    let (tx, rx) = channel();
    drop(tx);
    assert!(collect(vec![Request { id: "closed".into(), scope: scope(), reply: rx }], Duration::ZERO).is_err());
}

#[test]
fn a_resize_then_inspection_before_paint_cannot_relabel_previous_geometry() {
    let (tx, rx) = channel();
    tx.send(Ok(snapshot())).unwrap();
    let requested = Scope { revision: 2, size: Some((300.0, 400.0)), ..scope() };
    assert!(collect(vec![Request { id: "app".into(), scope: requested, reply: rx }], Duration::ZERO).is_err());
}
