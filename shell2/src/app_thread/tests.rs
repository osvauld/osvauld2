use super::*;

#[test]
fn a_beat_is_stuck_only_while_busy_past_the_limit() {
    let beat = Beat::default();
    let limit = Duration::from_millis(30);
    assert!(!beat.stuck_after(limit), "idle is never stuck");
    beat.start();
    assert!(!beat.stuck_after(limit));
    std::thread::sleep(limit * 2);
    assert!(beat.stuck_after(limit));
    beat.flagged.store(true, Ordering::SeqCst);
    assert!(beat.end(), "the flagged thread's idle tells the shell");
    assert!(!beat.stuck_after(limit));
    assert!(!beat.end(), "told once");
}

fn live_handle() -> AppThread {
    let (tx, _) = channel();
    let (done_tx, done) = channel();
    done_tx.send(()).unwrap();
    AppThread {
        instance: NEXT_INSTANCE.fetch_add(1, Ordering::Relaxed), layout_revision: 0, tx, ws_id: "ws".into(),
        frame: TileFrame::default(), latest: Arc::new(Mutex::new(None)),
        closed: Arc::new(AtomicBool::new(false)), done, offscreen: false, shown: true,
        size: Some((400.0, 300.0)), doc_names: Vec::new(), beat: Arc::new(Beat::default()), sound: None,
    }
}

#[test]
fn new_live_paint_preserves_requested_geometry_without_reusing_the_old_scene() {
    let mut app = live_handle();
    app.frame.rects = Arc::from([runtime::ElRect { id: "fire".into(), x: 10.0, y: 20.0,
        w: 50.0, h: 50.0, hits: vec!["touch"] }]);
    let incoming = TileFrame::default();
    let scene = incoming.scene.clone();
    *app.latest.lock().unwrap() = Some(incoming);
    assert!(app.take(None));
    assert!(Arc::ptr_eq(&app.frame.scene, &scene));
    assert_eq!(app.frame.rects[0].id, "fire");
    assert_eq!(app.frame.rects[0].x, 10.0);
}

#[test]
fn geometry_scope_changes_after_resize_or_reopen() {
    let mut app = live_handle();
    let scope = app.rect_scope();
    app.resize((300.0, 400.0), 0.0);
    assert_ne!(scope, app.rect_scope());
    assert_ne!(scope, live_handle().rect_scope());
}

#[test]
fn geometry_scope_rejects_a_resize_away_and_back() {
    let mut app = live_handle();
    let before = app.rect_scope();
    app.resize((300.0, 400.0), 0.0);
    app.resize((400.0, 300.0), 0.0);
    assert_ne!(before, app.rect_scope(), "a previous size must not revive an old observation");
}
