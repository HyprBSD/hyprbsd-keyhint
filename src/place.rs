//! Where the card goes: inside the bottom-right corner of the program's
//! window, from Hyprland's window and monitor lists.

use serde_json::Value;

/// A rectangle in Hyprland's layout coordinates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

impl Rect {
    fn intersect(&self, other: &Rect) -> Option<Rect> {
        let (x, y) = (self.x.max(other.x), self.y.max(other.y));
        let right = (self.x + self.w).min(other.x + other.w);
        let bottom = (self.y + self.h).min(other.y + other.h);
        (right > x && bottom > y).then_some(Rect { x, y, w: right - x, h: bottom - y })
    }
}

/// A window and the monitor it's on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Target {
    pub monitor: String,
    pub area: Rect,
    pub window: Rect,
}

/// The window at `address` and its monitor, from `j/clients` and
/// `j/monitors` replies; None when either can't be found.
pub fn target(clients: &str, monitors: &str, address: &str) -> Option<Target> {
    let clients: Vec<Value> = serde_json::from_str(clients).ok()?;
    let client = clients.iter().find(|client| client["address"] == address)?;
    let window = Rect {
        x: int(&client["at"][0])?,
        y: int(&client["at"][1])?,
        w: int(&client["size"][0])?,
        h: int(&client["size"][1])?,
    };
    let monitors: Vec<Value> = serde_json::from_str(monitors).ok()?;
    let monitor = monitors.iter().find(|monitor| monitor["id"].as_i64() == client["monitor"].as_i64())?;
    Some(Target { monitor: monitor["name"].as_str()?.to_owned(), area: logical_area(monitor)?, window })
}

/// The card's distance from its monitor's right and bottom edges that puts
/// it `inset` inside the bottom-right corner of the window's visible part;
/// None when a `card`-sized card doesn't fit there.
pub fn margins(target: &Target, card: (i32, i32), inset: i32) -> Option<(i32, i32)> {
    let visible = target.window.intersect(&target.area)?;
    if card.0 + 2 * inset > visible.w || card.1 + 2 * inset > visible.h {
        return None;
    }
    let area = &target.area;
    Some((area.x + area.w - (visible.x + visible.w) + inset, area.y + area.h - (visible.y + visible.h) + inset))
}

/// The name of the monitor in focus, from a `j/monitors` reply.
pub fn focused_monitor(monitors: &str) -> Option<String> {
    let monitors: Vec<Value> = serde_json::from_str(monitors).ok()?;
    let monitor = monitors.iter().find(|monitor| monitor["focused"].as_bool() == Some(true))?;
    Some(monitor["name"].as_str()?.to_owned())
}

/// A monitor's area in layout coordinates: its pixels, turned for an odd
/// transform, over its scale.
fn logical_area(monitor: &Value) -> Option<Rect> {
    let scale = monitor["scale"].as_f64().filter(|scale| *scale > 0.0)?;
    let (width, height) = (int(&monitor["width"])?, int(&monitor["height"])?);
    let (width, height) = if monitor["transform"].as_i64()? % 2 == 1 { (height, width) } else { (width, height) };
    Some(Rect {
        x: int(&monitor["x"])?,
        y: int(&monitor["y"])?,
        w: (f64::from(width) / scale).round() as i32,
        h: (f64::from(height) / scale).round() as i32,
    })
}

fn int(value: &Value) -> Option<i32> {
    value.as_i64()?.try_into().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    const MONITORS: &str = r#"[
        {"id": 0, "name": "HDMI-A-1", "x": 0, "y": 0, "width": 1920, "height": 1080, "scale": 1.0, "transform": 0},
        {"id": 1, "name": "DP-3", "x": 1920, "y": 0, "width": 3840, "height": 2160, "scale": 2.0, "transform": 0},
        {"id": 2, "name": "DP-4", "x": -2560, "y": 0, "width": 3840, "height": 2160, "scale": 1.5, "transform": 0},
        {"id": 3, "name": "DP-5", "x": 0, "y": 1080, "width": 1920, "height": 1080, "scale": 1.0, "transform": 1},
        {"id": 4, "name": "DP-6", "x": 0, "y": 3000, "width": 1920, "height": 1080, "scale": 1.0, "transform": 3},
        {"id": 5, "name": "DP-7", "x": 0, "y": 5000, "width": 1920, "height": 1080, "scale": 1.0, "transform": 5},
        {"id": 6, "name": "DP-8", "x": 0, "y": 7000, "width": 1920, "height": 1080, "scale": 1.0, "transform": 7}
    ]"#;

    fn clients(at: (i32, i32), size: (i32, i32), monitor: i32) -> String {
        format!(
            r#"[{{"address": "0x1", "at": [0, 0], "size": [10, 10], "monitor": 0}},
                {{"address": "0x2", "at": [{}, {}], "size": [{}, {}], "monitor": {monitor}}}]"#,
            at.0, at.1, size.0, size.1
        )
    }

    fn rect(x: i32, y: i32, w: i32, h: i32) -> Rect {
        Rect { x, y, w, h }
    }

    fn found(at: (i32, i32), size: (i32, i32), monitor: i32) -> Target {
        target(&clients(at, size, monitor), MONITORS, "0x2").unwrap()
    }

    #[test]
    fn target_should_give_the_window_and_its_monitor() {
        let found = found((100, 50), (800, 600), 0);
        assert_eq!(
            found,
            Target { monitor: "HDMI-A-1".into(), area: rect(0, 0, 1920, 1080), window: rect(100, 50, 800, 600) }
        );
    }

    #[test]
    fn a_monitor_at_scale_2_should_be_half_its_pixels() {
        assert_eq!(found((2000, 0), (10, 10), 1).area, rect(1920, 0, 1920, 1080));
    }

    #[test]
    fn a_monitor_at_scale_1_5_left_of_the_origin_should_keep_its_place() {
        assert_eq!(found((-2460, 100), (1200, 800), 2).area, rect(-2560, 0, 2560, 1440));
    }

    #[test]
    fn odd_transforms_should_swap_width_and_height() {
        let areas: Vec<(i32, i32)> =
            [3, 4, 5, 6].iter().map(|m| found((0, 0), (10, 10), *m).area).map(|a| (a.w, a.h)).collect();
        assert_eq!(areas, [(1080, 1920); 4]);
    }

    #[test]
    fn a_missing_window_should_give_nothing() {
        assert_eq!(target(&clients((0, 0), (10, 10), 0), MONITORS, "0x9"), None);
    }

    #[test]
    fn a_window_on_an_unknown_monitor_should_give_nothing() {
        assert_eq!(target(&clients((0, 0), (10, 10), 9), MONITORS, "0x2"), None);
    }

    #[test]
    fn replies_that_arent_lists_should_give_nothing() {
        assert_eq!(target("unknown request", MONITORS, "0x2"), None);
    }

    #[test]
    fn focused_monitor_should_name_the_one_in_focus() {
        let monitors = r#"[{"name": "HDMI-A-1", "focused": false}, {"name": "DP-3", "focused": true}]"#;
        assert_eq!(focused_monitor(monitors).as_deref(), Some("DP-3"));
    }

    #[test]
    fn focused_monitor_should_give_nothing_when_none_is() {
        assert_eq!(focused_monitor(r#"[{"name": "HDMI-A-1", "focused": false}]"#), None);
    }

    #[test]
    fn the_card_should_sit_inside_the_window_s_bottom_right_corner() {
        assert_eq!(margins(&found((100, 50), (800, 600), 0), (300, 60), 24), Some((1920 - 900 + 24, 1080 - 650 + 24)));
    }

    #[test]
    fn margins_should_be_from_the_monitor_s_own_edges() {
        assert_eq!(margins(&found((-2460, 100), (1200, 800), 2), (300, 60), 24), Some((1284, 564)));
    }

    #[test]
    fn a_window_partly_off_its_monitor_should_get_the_card_in_its_visible_part() {
        assert_eq!(margins(&found((1500, 900), (800, 600), 0), (300, 60), 24), Some((24, 24)));
    }

    #[test]
    fn a_window_too_small_for_the_card_should_give_nothing() {
        assert_eq!(margins(&found((100, 100), (300, 80), 0), (300, 60), 24), None);
    }

    #[test]
    fn a_window_off_its_monitor_should_give_nothing() {
        assert_eq!(margins(&found((3000, 0), (800, 600), 0), (300, 60), 24), None);
    }
}
