use super::*;

#[test]
fn accepts_every_bezier_verb_and_records_true_bounds() {
    let path = Path::new(vec![
        PathEl::MoveTo((10.0, 20.0).into()),
        PathEl::LineTo((30.0, 40.0).into()),
        PathEl::QuadTo((50.0, -20.0).into(), (70.0, 40.0).into()),
        PathEl::CurveTo(
            (80.0, 50.0).into(),
            (90.0, 60.0).into(),
            (100.0, 20.0).into(),
        ),
        PathEl::ClosePath,
    ])
    .unwrap();

    assert_eq!(path.command_count(), 5);
    assert_eq!(path.bezier().elements().len(), 5);
    assert_eq!(path.bounds(), path.bezier().bounding_box());
    assert!(path.bounds().y0 < 20.0); // the quadratic's internal extremum is included
}

#[test]
fn stroke_style_preserves_valid_caps_joins_and_dashes() {
    let style = StrokeStyle::new(
        3.5,
        StrokeCap::Square,
        StrokeJoin::Miter,
        6.0,
        vec![8.0, 3.0, 2.0],
        -1.5,
    )
    .unwrap();

    assert_eq!(style.0.width, 3.5);
    assert_eq!(style.0.start_cap, Cap::Square);
    assert_eq!(style.0.end_cap, Cap::Square);
    assert_eq!(style.0.join, Join::Miter);
    assert_eq!(style.0.miter_limit, 6.0);
    assert_eq!(style.0.dash_pattern.as_slice(), &[8.0, 3.0, 2.0]);
    assert_eq!(style.0.dash_offset, -1.5);
}

#[test]
fn stroke_item_is_budgeted_and_rendered_through_its_transform() {
    let path = dot(3);
    let style = StrokeStyle::new(
        2.0,
        StrokeCap::Round,
        StrokeJoin::Bevel,
        4.0,
        vec![5.0, 2.0],
        1.0,
    )
    .unwrap();
    let item = Item::stroke(path, Arc::new(Brush::solid(Color::WHITE).unwrap()), style);
    let frame = Frame::new(
        10.0,
        10.0,
        None,
        vec![Item::group(Affine::translate((3.0, 4.0)), vec![item]).unwrap()],
    )
    .unwrap();
    let mut scene = Scene::new();

    frame.draw(&mut scene, Affine::scale(2.0), 0.5);

    assert_eq!(frame.stats().expanded_path_commands, 3);
    assert_eq!(scene.encoding().n_paths, 1);
    assert_eq!(scene.encoding().styles.len(), 1);
    assert_eq!(scene.encoding().draw_tags.len(), 1);
    assert_eq!(
        scene.encoding().transforms[0].to_kurbo(),
        Affine::scale(2.0) * Affine::translate((3.0, 4.0))
    );
}

#[test]
fn stroke_style_rejects_unbounded_or_degenerate_values() {
    for width in [0.0, -1.0, f64::INFINITY, MAX_FRAME_COORDINATE + 1.0] {
        assert_eq!(
            StrokeStyle::new(width, StrokeCap::Butt, StrokeJoin::Bevel, 4.0, vec![], 0.0)
                .unwrap_err(),
            FrameError::InvalidStrokeWidth
        );
    }
    assert_eq!(
        StrokeStyle::new(1.0, StrokeCap::Round, StrokeJoin::Round, 0.5, vec![], 0.0).unwrap_err(),
        FrameError::InvalidMiterLimit
    );
    for (dashes, offset) in [
        (vec![0.0, 0.0], 0.0),
        (vec![-1.0, 2.0], 0.0),
        (vec![1.0], f64::NAN),
    ] {
        assert_eq!(
            StrokeStyle::new(
                1.0,
                StrokeCap::Round,
                StrokeJoin::Round,
                4.0,
                dashes,
                offset
            )
            .unwrap_err(),
            FrameError::InvalidDashPattern
        );
    }
    assert_eq!(
        StrokeStyle::new(
            1.0,
            StrokeCap::Round,
            StrokeJoin::Round,
            4.0,
            vec![1.0; MAX_STROKE_DASHES + 1],
            0.0,
        )
        .unwrap_err(),
        FrameError::TooManyStrokeDashes
    );
}

#[test]
fn empty_path_is_a_valid_resource() {
    let path = Path::new(Vec::new()).unwrap();
    assert_eq!(path.command_count(), 0);
}

#[test]
fn drawable_commands_require_a_fresh_move_after_close() {
    for elements in [
        vec![PathEl::LineTo((1.0, 1.0).into())],
        vec![
            PathEl::MoveTo((0.0, 0.0).into()),
            PathEl::ClosePath,
            PathEl::LineTo((1.0, 1.0).into()),
        ],
    ] {
        assert!(matches!(
            Path::new(elements),
            Err(PathError::InvalidSequence { .. })
        ));
    }
}

#[test]
fn rejects_non_finite_and_out_of_range_coordinates() {
    for point in [
        Point::new(f64::NAN, 0.0),
        Point::new(0.0, f64::INFINITY),
        Point::new(MAX_FRAME_COORDINATE + 1.0, 0.0),
    ] {
        assert_eq!(
            Path::new(vec![PathEl::MoveTo(point)]).unwrap_err(),
            PathError::InvalidCoordinate { index: 0 }
        );
    }
}

#[test]
fn rejects_a_resource_over_the_command_budget() {
    let mut elements = vec![PathEl::MoveTo((0.0, 0.0).into())];
    elements.resize(MAX_PATH_COMMANDS + 1, PathEl::LineTo((1.0, 1.0).into()));

    assert_eq!(
        Path::new(elements).unwrap_err(),
        PathError::TooManyCommands {
            count: MAX_PATH_COMMANDS + 1
        }
    );
}

fn dot(commands: usize) -> Arc<Path> {
    let mut elements = vec![PathEl::MoveTo((0.0, 0.0).into())];
    elements.resize(commands, PathEl::LineTo((1.0, 1.0).into()));
    Arc::new(Path::new(elements).unwrap())
}

fn fill(path: Arc<Path>) -> Item {
    Item::fill(
        path,
        Arc::new(Brush::solid(Color::WHITE).unwrap()),
        Fill::NonZero,
    )
}

#[test]
fn frame_records_intrinsics_and_expanded_nested_work() {
    let path = dot(2);
    let shared = Arc::new(Frame::new(10.0, 20.0, Some(15.0), vec![fill(path.clone())]).unwrap());
    let group = Item::group(Affine::translate((2.0, 3.0)), vec![fill(path.clone())]).unwrap();
    let instance = Item::instance(Affine::scale(2.0), shared).unwrap();
    let frame = Frame::new(100.0, 80.0, Some(60.0), vec![fill(path), group, instance]).unwrap();

    assert_eq!(frame.size(), (100.0, 80.0));
    assert_eq!(frame.baseline(), Some(60.0));
    assert_eq!(frame.items().len(), 3);
    assert_eq!(
        frame.stats(),
        FrameStats {
            expanded_items: 5,
            expanded_path_commands: 6,
            depth: 1,
            hittable: 0
        }
    );
}

#[test]
fn a_name_rides_on_the_item_and_is_counted() {
    let path = dot(2);
    let frame = Frame::new(
        10.0,
        10.0,
        None,
        vec![fill(path.clone()), fill(path).with_id("slice:1")],
    )
    .unwrap();

    assert_eq!(frame.items()[0].id(), None);
    assert_eq!(frame.items()[1].id().map(|id| &**id), Some("slice:1"));
    assert_eq!(frame.stats().hittable, 1);
}

/// The granularity rule, from both ends: a named container answers for its contents, and a
/// reused visual never lends its names to the frame that instances it.
#[test]
fn a_named_container_hides_the_names_inside_it() {
    let path = dot(2);
    let inner = || vec![fill(path.clone()).with_id("tick"), fill(path.clone())];

    let loose = Item::group(Affine::IDENTITY, inner()).unwrap();
    let dial = Item::group(Affine::IDENTITY, inner())
        .unwrap()
        .with_id("dial");
    let shared = Arc::new(Frame::new(10.0, 10.0, None, inner()).unwrap());
    let anonymous = Item::instance(Affine::IDENTITY, shared.clone()).unwrap();
    let named = Item::instance(Affine::IDENTITY, shared)
        .unwrap()
        .with_id("pin:7");

    let count = |item| {
        Frame::new(10.0, 10.0, None, vec![item])
            .unwrap()
            .stats()
            .hittable
    };
    assert_eq!(count(loose), 1); // the tick is reachable
    assert_eq!(count(dial), 1); // the dial is, the tick inside it isn't
    assert_eq!(count(anonymous), 0); // nothing inside an instance is nameable
    assert_eq!(count(named), 1);
}

#[test]
fn frame_rejects_invalid_intrinsics_transforms_and_colors() {
    assert_eq!(
        Frame::new(-1.0, 1.0, None, vec![]).unwrap_err(),
        FrameError::InvalidSize
    );
    assert_eq!(
        Frame::new(1.0, 1.0, Some(2.0), vec![]).unwrap_err(),
        FrameError::InvalidBaseline
    );
    assert_eq!(
        Item::group(Affine::new([f64::NAN; 6]), vec![]).unwrap_err(),
        FrameError::InvalidTransform
    );
    let mut color = Color::WHITE;
    color.components[0] = f32::NAN;
    assert_eq!(Brush::solid(color).unwrap_err(), FrameError::InvalidColor);
}

fn stop(offset: f32) -> GradientStop {
    GradientStop::new(offset, Color::WHITE).unwrap()
}

#[test]
fn linear_gradient_accepts_ordered_stops_and_hard_edges() {
    assert!(
        Brush::linear(
            Point::new(0.0, 0.0),
            Point::new(100.0, 50.0),
            vec![stop(0.0), stop(0.5), stop(0.5), stop(1.0)],
            Extend::Reflect,
        )
        .is_ok()
    );
}

#[test]
fn gradient_rejects_bad_stops_and_geometry() {
    assert_eq!(
        GradientStop::new(f32::NAN, Color::WHITE).unwrap_err(),
        FrameError::InvalidGradientStop
    );
    assert_eq!(
        Brush::linear(
            Point::ZERO,
            Point::new(1.0, 0.0),
            vec![stop(0.5)],
            Extend::Pad
        )
        .unwrap_err(),
        FrameError::InvalidGradientStops
    );
    assert_eq!(
        Brush::linear(
            Point::ZERO,
            Point::new(1.0, 0.0),
            vec![stop(0.8), stop(0.2)],
            Extend::Pad
        )
        .unwrap_err(),
        FrameError::InvalidGradientStops
    );
    assert_eq!(
        Brush::linear(
            Point::ZERO,
            Point::ZERO,
            vec![stop(0.0), stop(1.0)],
            Extend::Repeat
        )
        .unwrap_err(),
        FrameError::InvalidGradientGeometry
    );
    assert_eq!(
        Brush::linear(
            Point::ZERO,
            Point::new(1.0, 0.0),
            vec![stop(0.0); MAX_GRADIENT_STOPS + 1],
            Extend::Pad,
        )
        .unwrap_err(),
        FrameError::InvalidGradientStops
    );
}

#[test]
fn frame_budgets_expanded_items_and_path_commands() {
    let item = fill(dot(1));
    assert_eq!(
        Frame::new(1.0, 1.0, None, vec![item; MAX_FRAME_ITEMS + 1]).unwrap_err(),
        FrameError::TooManyItems
    );

    let path = dot(MAX_PATH_COMMANDS / 2 + 1);
    assert_eq!(
        Frame::new(1.0, 1.0, None, vec![fill(path.clone()), fill(path)]).unwrap_err(),
        FrameError::TooManyPathCommands
    );
}

#[test]
fn frame_rejects_excessive_group_depth() {
    let mut item = fill(dot(1));
    for _ in 0..=MAX_FRAME_DEPTH {
        item = Item::group(Affine::IDENTITY, vec![item]).unwrap();
    }
    assert_eq!(
        Frame::new(1.0, 1.0, None, vec![item]).unwrap_err(),
        FrameError::TooDeep
    );
}

#[test]
fn repeated_instances_cannot_bypass_expanded_budgets() {
    let leaf = Arc::new(Frame::new(1.0, 1.0, None, vec![fill(dot(1))]).unwrap());
    let instance = Item::instance(Affine::IDENTITY, leaf).unwrap();
    assert_eq!(
        Frame::new(1.0, 1.0, None, vec![instance; MAX_FRAME_ITEMS / 2 + 1]).unwrap_err(),
        FrameError::TooManyItems
    );

    let costly =
        Arc::new(Frame::new(1.0, 1.0, None, vec![fill(dot(MAX_PATH_COMMANDS / 2 + 1))]).unwrap());
    let instance = Item::instance(Affine::IDENTITY, costly).unwrap();
    assert_eq!(
        Frame::new(1.0, 1.0, None, vec![instance.clone(), instance]).unwrap_err(),
        FrameError::TooManyPathCommands
    );
}

#[test]
fn an_instance_counts_its_referenced_frames_depth() {
    let mut item = fill(dot(1));
    for _ in 0..MAX_FRAME_DEPTH {
        item = Item::group(Affine::IDENTITY, vec![item]).unwrap();
    }
    let deep = Arc::new(Frame::new(1.0, 1.0, None, vec![item]).unwrap());
    let instance = Item::instance(Affine::IDENTITY, deep).unwrap();

    assert_eq!(
        Frame::new(1.0, 1.0, None, vec![instance]).unwrap_err(),
        FrameError::TooDeep
    );
}

#[test]
fn renderer_emits_fills_for_nested_groups_and_instances() {
    let path = dot(2);
    let gradient = Arc::new(
        Brush::linear(
            Point::ZERO,
            Point::new(10.0, 0.0),
            vec![stop(0.0), stop(1.0)],
            Extend::Pad,
        )
        .unwrap(),
    );
    let nested = Item::fill(path.clone(), gradient, Fill::EvenOdd);
    let leaf = Arc::new(Frame::new(2.0, 2.0, None, vec![fill(path.clone())]).unwrap());
    let frame = Frame::new(
        20.0,
        20.0,
        None,
        vec![
            fill(path),
            Item::group(Affine::translate((3.0, 4.0)), vec![nested]).unwrap(),
            Item::instance(Affine::scale(2.0), leaf).unwrap(),
        ],
    )
    .unwrap();
    let mut scene = Scene::new();

    frame.draw(&mut scene, Affine::translate((7.0, 8.0)), 1.0);

    assert_eq!(scene.encoding().n_paths, 3);
    assert_eq!(scene.encoding().draw_tags.len(), 3);
    assert_eq!(scene.encoding().resources.color_stops.len(), 2);
    let transforms: Vec<_> = scene
        .encoding()
        .transforms
        .iter()
        .map(|transform| transform.to_kurbo())
        .collect();
    assert_eq!(
        transforms,
        vec![
            Affine::translate((7.0, 8.0)),
            Affine::translate((10.0, 12.0)),
            Affine::translate((7.0, 8.0)) * Affine::scale(2.0),
        ]
    );
}
