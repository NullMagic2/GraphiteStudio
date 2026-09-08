//! Recover only this app's window after a virtual/physical display disappears.
//! All geometry is native physical pixels; work areas exclude the taskbar.
use std::time::{Duration, Instant};
use windows_sys::Win32::Graphics::Gdi::{GetMonitorInfoW, MONITORINFO};
use winit::{
    dpi::{PhysicalPosition, PhysicalSize},
    platform::windows::MonitorHandleExtWindows,
    window::Window,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Rect {
    x: i32,
    y: i32,
    width: i32,
    height: i32,
}
#[derive(Clone, Debug)]
struct Display {
    id: String,
    work: Rect,
    primary: bool,
}

fn title_is_reachable(window: Rect, screen: Rect) -> bool {
    let left = window.x.max(screen.x);
    let right = (window.x + window.width).min(screen.x + screen.width);
    let top = window.y.max(screen.y);
    let bottom = (window.y + 40).min(screen.y + screen.height);
    right - left >= 160.min(window.width) && bottom - top >= 24
}
fn fit(window: Rect, screen: Rect) -> Rect {
    let width = window.width.max(640).min((screen.width - 24).max(1));
    let height = window.height.max(480).min((screen.height - 24).max(1));
    Rect {
        x: screen.x + (screen.width - width) / 2,
        y: screen.y + (screen.height - height) / 2,
        width,
        height,
    }
}

#[derive(Default)]
pub struct WindowRecovery {
    force_recovery: bool,
    last_check: Option<Instant>,
    owner: Option<String>,
    was_minimized: bool,
    last_visible: Option<Rect>,
    pending_restore: bool,
    stranded_samples: u8,
}

impl WindowRecovery {
    pub fn request_recovery(&mut self) {
        self.force_recovery = true;
    }
    fn observe(&mut self, rect: Rect, screens: &[Display], minimized: bool) -> Option<Rect> {
        if screens.is_empty() {
            return None;
        } // transient display reconfiguration
        let owner_lost = self
            .owner
            .as_ref()
            .is_some_and(|id| !screens.iter().any(|s| &s.id == id));
        // Windows may minimize a visible window automatically when its display disconnects.
        // A window that was already intentionally minimized stays minimized.
        if minimized && !self.was_minimized && owner_lost {
            self.pending_restore = true;
        }
        self.was_minimized = minimized;
        if minimized && !self.pending_restore {
            return None;
        }
        if !minimized {
            self.last_visible = Some(rect);
            if let Some(screen) = screens.iter().find(|s| title_is_reachable(rect, s.work)) {
                self.owner = Some(screen.id.clone());
                self.stranded_samples = 0;
                self.pending_restore = false;
                return None;
            }
        }
        self.stranded_samples = self.stranded_samples.saturating_add(1);
        if self.stranded_samples < 2 {
            return None;
        } // allow Windows to finish its own move first
        let screen = screens.iter().find(|s| s.primary).unwrap_or(&screens[0]);
        let target = fit(self.last_visible.unwrap_or(rect), screen.work);
        self.owner = Some(screen.id.clone());
        self.stranded_samples = 0;
        self.pending_restore = false;
        Some(target)
    }

    pub fn tick(&mut self, window: &Window) -> bool {
        if !self.force_recovery
            && self
                .last_check
                .is_some_and(|t| t.elapsed() < Duration::from_millis(700))
        {
            return false;
        }
        self.last_check = Some(Instant::now());
        let screens: Vec<_> = window
            .available_monitors()
            .filter_map(|monitor| {
                let mut info: MONITORINFO = unsafe { std::mem::zeroed() };
                info.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
                // SAFETY: winit supplies a live monitor handle; info has the documented size.
                if unsafe { GetMonitorInfoW(monitor.hmonitor() as _, &mut info) } == 0 {
                    return None;
                }
                let r = info.rcWork;
                Some(Display {
                    id: monitor.native_id(),
                    primary: info.dwFlags & 1 != 0,
                    work: Rect {
                        x: r.left,
                        y: r.top,
                        width: r.right - r.left,
                        height: r.bottom - r.top,
                    },
                })
            })
            .collect();
        let Ok(position) = window.outer_position() else {
            return false;
        };
        let size = window.outer_size();
        let rect = Rect {
            x: position.x,
            y: position.y,
            width: size.width as i32,
            height: size.height as i32,
        };
        let target = if self.force_recovery {
            screens
                .iter()
                .find(|s| s.primary)
                .or_else(|| screens.first())
                .map(|screen| fit(self.last_visible.unwrap_or(rect), screen.work))
        } else {
            self.observe(rect, &screens, window.is_minimized().unwrap_or(false))
        };
        if let Some(target) = target {
            self.force_recovery = false;
            window.set_fullscreen(None);
            window.set_minimized(false);
            window.set_maximized(false);
            // Relax only the window minimum to accommodate a smaller/high-DPI fallback display.
            // The document and its resolution are untouched.
            let inner = PhysicalSize::new(
                (target.width - 24).max(1) as u32,
                (target.height - 56).max(1) as u32,
            );
            window.set_min_inner_size(Some(PhysicalSize::new(
                inner.width.min(640),
                inner.height.min(480),
            )));
            let _ = window.request_inner_size(inner);
            window.set_outer_position(PhysicalPosition::new(target.x, target.y));
            window.set_visible(true);
            return true;
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn main_display() -> Display {
        Display {
            id: "main".into(),
            primary: true,
            work: Rect {
                x: 0,
                y: 0,
                width: 1920,
                height: 1040,
            },
        }
    }
    fn ipad() -> Display {
        Display {
            id: "ipad".into(),
            primary: false,
            work: Rect {
                x: -2048,
                y: 0,
                width: 2048,
                height: 1536,
            },
        }
    }
    fn window() -> Rect {
        Rect {
            x: -1900,
            y: 80,
            width: 1600,
            height: 1000,
        }
    }
    #[test]
    fn ipad_disconnect_moves_to_remaining_work_area() {
        let mut state = WindowRecovery::default();
        assert!(state
            .observe(window(), &[main_display(), ipad()], false)
            .is_none());
        assert!(state.observe(window(), &[main_display()], false).is_none());
        let target = state.observe(window(), &[main_display()], false).unwrap();
        assert!(title_is_reachable(target, main_display().work));
        assert!(target.x >= 0 && target.y >= 0 && target.y + target.height <= 1040);
    }
    #[test]
    fn disconnect_restores_automatically_minimized_window() {
        let mut state = WindowRecovery::default();
        state.observe(window(), &[main_display(), ipad()], false);
        let iconic = Rect {
            x: -32000,
            y: -32000,
            width: 160,
            height: 28,
        };
        assert!(state.observe(iconic, &[main_display()], true).is_none());
        assert!(state.observe(iconic, &[main_display()], true).is_some());
    }
    #[test]
    fn intentional_minimize_and_valid_negative_coordinates_are_left_alone() {
        let mut state = WindowRecovery::default();
        for _ in 0..4 {
            assert!(state
                .observe(window(), &[main_display(), ipad()], false)
                .is_none());
        }
        state.observe(window(), &[main_display(), ipad()], true);
        for _ in 0..4 {
            assert!(state.observe(window(), &[main_display()], true).is_none());
        }
    }
    #[test]
    fn partial_window_with_unreachable_title_and_small_fallback_are_recovered() {
        let mut state = WindowRecovery::default();
        let small = Display {
            id: "small".into(),
            primary: true,
            work: Rect {
                x: 1920,
                y: 0,
                width: 800,
                height: 560,
            },
        };
        let bad = Rect {
            x: 1950,
            y: -700,
            width: 2500,
            height: 2000,
        };
        assert!(state.observe(bad, &[], false).is_none());
        state.observe(bad, &[small.clone()], false);
        let target = state.observe(bad, &[small.clone()], false).unwrap();
        assert!(target.width < 800 && target.height < 560);
        assert!(title_is_reachable(target, small.work));
    }
}
