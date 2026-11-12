//! Saved main-window geometry and the rules for when it is safe to reuse it.
//!
//! Windows reports a minimized window as 0x0 at (-32000, -32000). v0.1.1 saved
//! that geometry and restored it on the next launch, so the window opened
//! off-screen and the app looked like it never started. Geometry is now only
//! saved from a normal, visible window, and is only restored when it lands on a
//! connected monitor; otherwise the window opens centred at its default size.

use serde::{Deserialize, Serialize};

pub const DEFAULT_WIDTH: u32 = 1180;
pub const DEFAULT_HEIGHT: u32 = 760;
const MIN_WIDTH: u32 = 400;
const MIN_HEIGHT: u32 = 300;
/// How much of the title-bar area must be on a monitor for the window to count as reachable.
const MIN_VISIBLE: i64 = 100;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct WindowState {
    pub width: u32,
    pub height: u32,
    pub x: Option<i32>,
    pub y: Option<i32>,
    #[serde(default)]
    pub maximized: bool,
}

/// A monitor's area in physical pixels: x, y, width, height.
pub type MonitorRect = (i32, i32, u32, u32);

impl WindowState {
    pub fn size_is_usable(&self) -> bool {
        self.width >= MIN_WIDTH && self.height >= MIN_HEIGHT
    }

    /// True when the saved position puts a usable part of the window on one of `monitors`.
    pub fn position_is_on_screen(&self, monitors: &[MonitorRect]) -> bool {
        let (Some(x), Some(y)) = (self.x, self.y) else {
            return false;
        };
        let (x, y) = (x as i64, y as i64);
        let w = self.width as i64;
        // Only the top strip matters: that is where the user grabs the window.
        let (top, bottom) = (y, y + 40);
        monitors.iter().any(|&(mx, my, mw, mh)| {
            let (mx, my, mw, mh) = (mx as i64, my as i64, mw as i64, mh as i64);
            let overlap_w = (x + w).min(mx + mw) - x.max(mx);
            let top_inside = top >= my - 20 && top < my + mh && bottom > my;
            overlap_w >= MIN_VISIBLE && top_inside
        })
    }

    /// Whether this geometry is worth writing to disk at all.
    pub fn is_worth_saving(&self) -> bool {
        if !self.size_is_usable() {
            return false;
        }
        // Windows parks minimized windows at -32000.
        !matches!((self.x, self.y), (Some(x), Some(y)) if x <= -30000 || y <= -30000)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SCREEN: MonitorRect = (0, 0, 1920, 1080);

    fn ws(width: u32, height: u32, x: i32, y: i32) -> WindowState {
        WindowState { width, height, x: Some(x), y: Some(y), maximized: false }
    }

    #[test]
    fn minimized_geometry_from_v011_is_rejected() {
        let saved = ws(0, 0, -32000, -32000);
        assert!(!saved.is_worth_saving());
        assert!(!saved.size_is_usable());
        assert!(!saved.position_is_on_screen(&[SCREEN]));
    }

    #[test]
    fn v011_file_still_parses() {
        let text = r#"{"width":0,"height":0,"x":-32000,"y":-32000}"#;
        let s: WindowState = serde_json::from_str(text).unwrap();
        assert!(!s.maximized);
        assert!(!s.size_is_usable());
    }

    #[test]
    fn normal_window_is_kept() {
        let saved = ws(1180, 760, 200, 100);
        assert!(saved.is_worth_saving());
        assert!(saved.position_is_on_screen(&[SCREEN]));
    }

    #[test]
    fn maximized_border_offset_counts_as_on_screen() {
        assert!(ws(1920, 991, -9, -9).position_is_on_screen(&[SCREEN]));
    }

    #[test]
    fn window_on_unplugged_second_monitor_is_off_screen() {
        assert!(!ws(1180, 760, 2500, 100).position_is_on_screen(&[SCREEN]));
        assert!(ws(1180, 760, 2500, 100).position_is_on_screen(&[SCREEN, (1920, 0, 2560, 1440)]));
    }

    #[test]
    fn missing_position_or_no_monitors_is_off_screen() {
        let mut s = ws(1180, 760, 0, 0);
        s.x = None;
        assert!(!s.position_is_on_screen(&[SCREEN]));
        assert!(!ws(1180, 760, 10, 10).position_is_on_screen(&[]));
    }
}
