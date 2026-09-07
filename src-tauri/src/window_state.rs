use crate::settings::{AppSettings, SettingsStore, WindowBounds};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
use tauri::{LogicalSize, Manager, Monitor, PhysicalPosition, WebviewWindow};

pub const DEFAULT_WIDTH: f64 = 380.0;
pub const DEFAULT_HEIGHT: f64 = 720.0;
pub const EXPANDED_MIN_WIDTH: f64 = 300.0;
pub const EXPANDED_MIN_HEIGHT: f64 = 140.0;
pub const EXPANDED_MAX_WIDTH: f64 = 1200.0;
pub const EXPANDED_MAX_HEIGHT: f64 = 1400.0;
const PERSIST_DELAY: Duration = Duration::from_millis(400);

#[derive(Default)]
pub struct WindowBoundsController {
    generation: AtomicU64,
}

pub fn restore_initial(window: &WebviewWindow, settings: &AppSettings) -> Result<(), String> {
    restore_expanded_constraints(window)?;
    let Some(bounds) = settings.window_bounds.map(normalize_bounds) else {
        return Ok(());
    };
    window
        .set_size(LogicalSize::new(bounds.width, bounds.height))
        .map_err(window_error)?;
    if saved_position_is_visible(window, bounds)? {
        window
            .set_position(PhysicalPosition::new(bounds.x, bounds.y))
            .map_err(window_error)?;
    }
    Ok(())
}

pub fn restore_expanded_constraints(window: &WebviewWindow) -> Result<(), String> {
    window
        .set_min_size(Some(LogicalSize::new(
            EXPANDED_MIN_WIDTH,
            EXPANDED_MIN_HEIGHT,
        )))
        .map_err(window_error)?;
    window
        .set_max_size(Some(LogicalSize::new(
            EXPANDED_MAX_WIDTH,
            EXPANDED_MAX_HEIGHT,
        )))
        .map_err(window_error)?;
    window.set_resizable(true).map_err(window_error)
}

pub fn schedule_persist(window: WebviewWindow) {
    let app = window.app_handle().clone();
    let generation = app
        .state::<WindowBoundsController>()
        .generation
        .fetch_add(1, Ordering::Relaxed)
        + 1;
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(PERSIST_DELAY).await;
        let controller = app.state::<WindowBoundsController>();
        if controller.generation.load(Ordering::Relaxed) != generation {
            return;
        }
        if let Some(window) = app.get_webview_window("main") {
            let _ = persist_now(&window);
        }
    });
}

pub fn persist_now(window: &WebviewWindow) -> Result<(), String> {
    let Some(bounds) = capture_expanded_bounds(window)? else {
        return Ok(());
    };
    window
        .app_handle()
        .state::<SettingsStore>()
        .update_window_bounds(bounds)
}

fn capture_expanded_bounds(window: &WebviewWindow) -> Result<Option<WindowBounds>, String> {
    if window.is_maximized().map_err(window_error)? {
        return Ok(None);
    }
    let scale = window.scale_factor().map_err(window_error)?;
    let position = window.outer_position().map_err(window_error)?;
    let size = window.outer_size().map_err(window_error)?;
    let logical = size.to_logical::<f64>(scale);
    if logical.width < EXPANDED_MIN_WIDTH || logical.height < EXPANDED_MIN_HEIGHT {
        return Ok(None);
    }
    Ok(Some(normalize_bounds(WindowBounds {
        x: position.x,
        y: position.y,
        width: logical.width,
        height: logical.height,
    })))
}

fn normalize_bounds(mut bounds: WindowBounds) -> WindowBounds {
    if !bounds.width.is_finite() {
        bounds.width = DEFAULT_WIDTH;
    }
    if !bounds.height.is_finite() {
        bounds.height = DEFAULT_HEIGHT;
    }
    bounds.width = bounds
        .width
        .round()
        .clamp(EXPANDED_MIN_WIDTH, EXPANDED_MAX_WIDTH);
    bounds.height = bounds
        .height
        .round()
        .clamp(EXPANDED_MIN_HEIGHT, EXPANDED_MAX_HEIGHT);
    bounds
}

fn saved_position_is_visible(window: &WebviewWindow, bounds: WindowBounds) -> Result<bool, String> {
    let monitors = window.available_monitors().map_err(window_error)?;
    Ok(monitors
        .iter()
        .any(|monitor| bounds_intersect_monitor(bounds, monitor)))
}

fn bounds_intersect_monitor(bounds: WindowBounds, monitor: &Monitor) -> bool {
    let work = monitor.work_area();
    let width = (bounds.width * monitor.scale_factor()).round().max(1.0) as i64;
    let height = (bounds.height * monitor.scale_factor()).round().max(1.0) as i64;
    rects_intersect(
        (bounds.x as i64, bounds.y as i64, width, height),
        (
            work.position.x as i64,
            work.position.y as i64,
            work.size.width as i64,
            work.size.height as i64,
        ),
    )
}

fn rects_intersect(left: (i64, i64, i64, i64), right: (i64, i64, i64, i64)) -> bool {
    left.0 + left.2 > right.0
        && left.0 < right.0 + right.2
        && left.1 + left.3 > right.1
        && left.1 < right.1 + right.3
}

fn window_error(error: tauri::Error) -> String {
    format!("window state operation failed: {error}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_saved_size_to_expanded_window_limits() {
        let bounds = normalize_bounds(WindowBounds {
            x: 10,
            y: 20,
            width: 10.0,
            height: 9000.0,
        });
        assert_eq!(bounds.width, EXPANDED_MIN_WIDTH);
        assert_eq!(bounds.height, EXPANDED_MAX_HEIGHT);
        assert_eq!(bounds.x, 10);
        assert_eq!(bounds.y, 20);
    }

    #[test]
    fn rectangle_visibility_requires_real_overlap() {
        assert!(rects_intersect((10, 10, 100, 100), (0, 0, 50, 50)));
        assert!(!rects_intersect((50, 0, 100, 100), (0, 0, 50, 50)));
        assert!(!rects_intersect((-100, -100, 20, 20), (0, 0, 50, 50)));
    }
}
