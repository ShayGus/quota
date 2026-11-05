use super::*;
use quota_contracts::commands::WidgetGrowth;

const fn rect(x: f64, y: f64, width: f64, height: f64) -> Rect {
    Rect {
        x,
        y,
        width,
        height,
    }
}

const MAIN: Rect = rect(0.0, 0.0, 1920.0, 1080.0);
const MAIN_WORK_AREA: Rect = rect(0.0, 0.0, 1920.0, 1032.0);
const LEFT: Rect = rect(-2560.0, -200.0, 2560.0, 1440.0);
const SIZE: (f64, f64) = (395.0, 128.0);

#[test]
fn a_first_run_puts_the_widget_in_the_top_right_corner() {
    assert_eq!(
        place(None, SIZE, &[MAIN], MAIN_WORK_AREA, 24.0),
        (1501.0, 24.0)
    );
}

#[test]
fn a_saved_position_on_any_screen_is_kept_exactly() {
    let saved = WidgetPosition { x: -1800, y: 40 };
    assert_eq!(
        place(Some(saved), SIZE, &[MAIN, LEFT], MAIN_WORK_AREA, 24.0),
        (-1800.0, 40.0)
    );
    let edge = WidgetPosition { x: 1700, y: 900 };
    assert_eq!(
        place(Some(edge), SIZE, &[MAIN], MAIN_WORK_AREA, 24.0),
        (1700.0, 900.0)
    );
}

#[test]
fn a_position_on_a_screen_that_is_gone_falls_back_to_the_corner() {
    let saved = WidgetPosition { x: -1800, y: 40 };
    assert_eq!(
        place(Some(saved), SIZE, &[MAIN], MAIN_WORK_AREA, 24.0),
        (1501.0, 24.0)
    );
}

#[test]
fn a_widget_whose_top_edge_is_off_screen_falls_back_to_the_corner() {
    let above = WidgetPosition { x: 400, y: -100 };
    assert_eq!(
        place(Some(above), SIZE, &[MAIN], MAIN_WORK_AREA, 24.0),
        (1501.0, 24.0)
    );
}

const AREA: WorkArea = WorkArea {
    top: 0,
    height: 1032,
};

const fn placed(y: i32, height: u32) -> Placement {
    Placement {
        outer_y: y,
        outer_height: height,
        inner_height: height,
    }
}

#[test]
fn a_growth_with_room_below_keeps_the_top_edge() {
    let fitted = fit(256.0, WidgetGrowth::Down, placed(24, 172), AREA, 1.0);
    assert!((fitted.height - 256.0).abs() < f64::EPSILON);
    assert_eq!(fitted.direction, WidgetGrowth::Down);
    assert_eq!(fitted.outer_y, 24);
    assert!((fitted.room_above - 24.0).abs() < f64::EPSILON);
    assert!((fitted.room_below - 836.0).abs() < f64::EPSILON);
}

#[test]
fn a_growth_without_room_below_opens_upward_and_keeps_the_bottom_edge() {
    let fitted = fit(256.0, WidgetGrowth::Down, placed(850, 172), AREA, 1.0);
    assert!((fitted.height - 256.0).abs() < f64::EPSILON);
    assert_eq!(fitted.direction, WidgetGrowth::Up);
    // The bottom edge stays at 1022, so the tiles keep their place.
    assert_eq!(fitted.outer_y, 766);
    assert!((fitted.room_below - 10.0).abs() < f64::EPSILON);
}

#[test]
fn a_downward_close_keeps_the_top_edge() {
    let fitted = fit(172.0, WidgetGrowth::Down, placed(24, 256), AREA, 1.0);
    assert!((fitted.height - 172.0).abs() < f64::EPSILON);
    assert_eq!(fitted.direction, WidgetGrowth::Down);
    assert_eq!(fitted.outer_y, 24);
}

#[test]
fn an_upward_close_moves_the_top_down_to_the_resting_position() {
    let fitted = fit(172.0, WidgetGrowth::Up, placed(766, 256), AREA, 1.0);
    assert!((fitted.height - 172.0).abs() < f64::EPSILON);
    assert_eq!(fitted.direction, WidgetGrowth::Up);
    assert_eq!(fitted.outer_y, 850);
}

#[test]
fn a_taller_switch_upward_stays_upward() {
    let fitted = fit(296.0, WidgetGrowth::Up, placed(766, 256), AREA, 1.0);
    assert!((fitted.height - 296.0).abs() < f64::EPSILON);
    assert_eq!(fitted.direction, WidgetGrowth::Up);
    assert_eq!(fitted.outer_y, 726);
}

#[test]
fn without_room_on_either_side_the_larger_side_wins_capped() {
    let area = WorkArea {
        top: 0,
        height: 200,
    };
    let fitted = fit(256.0, WidgetGrowth::Down, placed(10, 172), area, 1.0);
    assert_eq!(fitted.direction, WidgetGrowth::Down);
    assert!((fitted.height - 182.0).abs() < f64::EPSILON);
    assert_eq!(fitted.outer_y, 10);
    assert!(f64::from(fitted.outer_y) + fitted.height <= f64::from(area.height));
}

#[test]
fn capped_growth_stays_inside_the_work_area_in_both_directions_and_scales() {
    for scale in [1_u16, 2] {
        let area = WorkArea {
            top: 100,
            height: 480 * u32::from(scale),
        };
        for preferred in [WidgetGrowth::Down, WidgetGrowth::Up] {
            for (offset, direction, growth) in [
                (24, WidgetGrowth::Down, 24.0),
                (32, WidgetGrowth::Up, 24.0),
                (28, WidgetGrowth::Down, 20.0),
            ] {
                let placement =
                    placed(area.top + offset * i32::from(scale), 424 * u32::from(scale));
                let fitted = fit(600.0, preferred, placement, area, f64::from(scale));
                assert_eq!(fitted.direction, direction);
                assert!((fitted.height - (424.0 + growth)).abs() < f64::EPSILON);
                assert!(fitted.outer_y >= area.top);
                assert!(
                    f64::from(fitted.outer_y) + fitted.height * f64::from(scale)
                        <= f64::from(area.top) + f64::from(area.height)
                );
            }
        }
    }
}

#[test]
fn no_room_leaves_the_window_at_its_resting_size() {
    for preferred in [WidgetGrowth::Down, WidgetGrowth::Up] {
        let fitted = fit(
            256.0,
            preferred,
            placed(104, 172),
            WorkArea {
                top: 100,
                height: 180,
            },
            1.0,
        );
        assert!((fitted.height - 172.0).abs() < f64::EPSILON);
        assert_eq!(fitted.outer_y, 104);
    }
}

#[test]
fn a_large_resting_widget_grows_only_into_the_available_room() {
    let fitted = fit(
        532.0,
        WidgetGrowth::Down,
        placed(24, 424),
        WorkArea {
            top: 0,
            height: 480,
        },
        1.0,
    );
    assert!((fitted.height - 448.0).abs() < f64::EPSILON);
    assert_eq!(fitted.direction, WidgetGrowth::Down);
    assert_eq!(fitted.outer_y, 24);
    assert!(f64::from(fitted.outer_y) + fitted.height <= 480.0);
}

#[test]
fn opening_after_a_drag_below_the_work_area_moves_the_window_back_inside() {
    let fitted = fit(224.0, WidgetGrowth::Down, placed(950, 116), AREA, 1.0);
    assert!((fitted.height - 224.0).abs() < f64::EPSILON);
    assert_eq!(fitted.direction, WidgetGrowth::Up);
    assert_eq!(fitted.outer_y, 808);
}

#[test]
fn every_resize_keeps_both_edges_inside_with_scaled_window_frames() {
    for scale in [1_u16, 2] {
        let area = WorkArea {
            top: -200,
            height: 1032 * u32::from(scale),
        };
        for preferred in [WidgetGrowth::Down, WidgetGrowth::Up] {
            for offset in [-40, 950] {
                for current in [116, 1200] {
                    for content in [80.0, 224.0, 1400.0] {
                        let frame = 10 * u32::from(scale);
                        let placement = Placement {
                            outer_y: area.top + offset * i32::from(scale),
                            inner_height: current * u32::from(scale),
                            outer_height: current * u32::from(scale) + frame,
                        };
                        let fitted = fit(content, preferred, placement, area, f64::from(scale));
                        assert!(fitted.outer_y >= area.top);
                        assert!(
                            f64::from(fitted.outer_y)
                                + (fitted.height * f64::from(scale)).round()
                                + f64::from(frame)
                                <= f64::from(area.top) + f64::from(area.height)
                        );
                        assert!(fitted.height <= content);
                    }
                }
            }
        }
    }
}

#[test]
fn room_is_measured_in_logical_pixels() {
    let area = WorkArea {
        top: 0,
        height: 2064,
    };
    let fitted = fit(256.0, WidgetGrowth::Down, placed(48, 344), area, 2.0);
    assert!((fitted.height - 256.0).abs() < f64::EPSILON);
    assert_eq!(fitted.direction, WidgetGrowth::Down);
    assert_eq!(fitted.outer_y, 48);
    assert!((fitted.room_below - 836.0).abs() < f64::EPSILON);
}

#[test]
fn the_fitted_width_is_the_configured_widget_width() {
    let config: serde_json::Value =
        serde_json::from_str(include_str!("../../tauri.conf.json")).expect("the configuration");
    let windows = config["app"]["windows"].as_array().expect("the windows");
    let widget = windows
        .iter()
        .find(|window| window["label"] == LABEL)
        .expect("the widget window");
    assert_eq!(widget["width"].as_f64(), Some(WIDTH));
}

const SNAP_AREA: Rect = rect(0.0, 0.0, 1920.0, 1032.0);

fn snapped_to(x: f64, y: f64) -> (f64, f64) {
    snap((x, y), (200.0, 100.0), SNAP_AREA, 28.0, 8.0)
}

#[test]
fn a_widget_left_near_an_edge_snaps_to_it() {
    assert_eq!(snapped_to(20.0, 500.0), (8.0, 500.0));
    assert_eq!(snapped_to(1700.0, 500.0), (1712.0, 500.0));
    assert_eq!(snapped_to(800.0, 10.0), (800.0, 8.0));
    assert_eq!(snapped_to(800.0, 910.0), (800.0, 924.0));
    assert_eq!(snapped_to(10.0, 915.0), (8.0, 924.0));
}

#[test]
fn a_widget_left_away_from_the_edges_stays_exactly_there() {
    assert_eq!(snapped_to(800.0, 500.0), (800.0, 500.0));
    assert_eq!(snapped_to(28.0, 28.0), (28.0, 28.0));
}

#[test]
fn a_widget_partly_off_the_screen_snaps_back_onto_it() {
    assert_eq!(snapped_to(-50.0, 500.0), (8.0, 500.0));
    assert_eq!(snapped_to(1800.0, 500.0), (1712.0, 500.0));
}

fn assert_left(anchor: WidthAnchor, x: f64, current: f64, width: f64, expected: f64) {
    let left = fit_x(anchor, x, current, width, SNAP_AREA, 28.0);
    assert!(
        (left - expected).abs() < f64::EPSILON,
        "{left} != {expected}"
    );
}

#[test]
fn a_saved_spot_keeps_its_left_edge_when_the_window_narrows() {
    assert_left(WidthAnchor::Left, 1500.0, 316.0, 197.0, 1500.0);
}

#[test]
fn the_first_run_corner_keeps_its_right_edge_when_the_window_narrows() {
    assert_left(WidthAnchor::Right, 1580.0, 316.0, 197.0, 1699.0);
}

#[test]
fn a_widget_snapped_to_the_right_stays_there_as_its_width_changes() {
    assert_left(WidthAnchor::Resting, 1715.0, 197.0, 180.0, 1732.0);
    assert_left(WidthAnchor::Resting, 1715.0, 197.0, 240.0, 1672.0);
}

#[test]
fn a_widget_away_from_the_right_keeps_its_left_edge() {
    assert_left(WidthAnchor::Resting, 400.0, 197.0, 240.0, 400.0);
}

#[test]
fn a_widening_widget_is_kept_on_its_screen() {
    assert_left(WidthAnchor::Left, 1700.0, 197.0, 316.0, 1604.0);
}

#[test]
fn a_width_is_kept_between_the_narrowest_and_the_widest() {
    assert!((clamp_width(197.0) - 197.0).abs() < f64::EPSILON);
    assert!((clamp_width(10.0) - MIN_WIDTH).abs() < f64::EPSILON);
    assert!((clamp_width(900.0) - MAX_WIDTH).abs() < f64::EPSILON);
    assert!((clamp_width(f64::NAN) - WIDTH).abs() < f64::EPSILON);
}
