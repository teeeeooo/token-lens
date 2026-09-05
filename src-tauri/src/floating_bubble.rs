use crate::appearance;
use crate::settings::SettingsStore;
use serde::Serialize;
use std::sync::Mutex;
use tauri::{LogicalSize, Monitor, PhysicalPosition, PhysicalSize, WebviewWindow};

const BUBBLE_LOGICAL_SIZE: f64 = 34.0;
const EXPANDED_MIN_WIDTH: f64 = 240.0;
const EXPANDED_MIN_HEIGHT: f64 = 140.0;
const EXPANDED_MARGIN: i32 = 8;
const COLLAPSED_Y_MARGIN: i32 = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PlatformPolicy {
    Windows,
    Desktop,
}

impl PlatformPolicy {
    fn current() -> Self {
        if cfg!(target_os = "windows") {
            Self::Windows
        } else {
            Self::Desktop
        }
    }

    fn collapsed_y_margin(self) -> i32 {
        match self {
            Self::Windows => 0,
            Self::Desktop => COLLAPSED_Y_MARGIN,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum FloatingBubbleSide {
    Left,
    Right,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FloatingBubblePayload {
    pub enabled: bool,
    pub collapsed: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub side: Option<FloatingBubbleSide>,
}

#[derive(Debug, Clone, Copy)]
struct ExpandedState {
    logical_width: f64,
    logical_height: f64,
    always_on_top: bool,
}

#[derive(Debug, Default)]
struct BubbleState {
    collapsed: bool,
    side: Option<FloatingBubbleSide>,
    expanded: Option<ExpandedState>,
}

#[derive(Default)]
pub struct FloatingBubbleController {
    state: Mutex<BubbleState>,
}

#[derive(Debug, Clone, Copy, Default, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BubbleDragOffset {
    pub offset_ratio_x: Option<f64>,
    pub offset_ratio_y: Option<f64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Bounds {
    x: i32,
    y: i32,
    width: u32,
    height: u32,
}

impl Bounds {
    fn from_window(window: &WebviewWindow) -> Result<Self, String> {
        let position = window.outer_position().map_err(window_error)?;
        let size = window.outer_size().map_err(window_error)?;
        Ok(Self {
            x: position.x,
            y: position.y,
            width: size.width,
            height: size.height,
        })
    }
}
fn window_error(error: tauri::Error) -> String {
    format!("floating bubble window operation failed: {error}")
}

fn monitor_work_area(monitor: &Monitor) -> Bounds {
    let area = monitor.work_area();
    Bounds {
        x: area.position.x,
        y: area.position.y,
        width: area.size.width,
        height: area.size.height,
    }
}

fn monitor_full_area(monitor: &Monitor) -> Bounds {
    Bounds {
        x: monitor.position().x,
        y: monitor.position().y,
        width: monitor.size().width,
        height: monitor.size().height,
    }
}

fn select_collapsed_area(full_area: Bounds, work_area: Bounds, policy: PlatformPolicy) -> Bounds {
    match policy {
        PlatformPolicy::Windows => full_area,
        PlatformPolicy::Desktop => work_area,
    }
}

fn collapsed_area(monitor: &Monitor, policy: PlatformPolicy) -> Bounds {
    select_collapsed_area(
        monitor_full_area(monitor),
        monitor_work_area(monitor),
        policy,
    )
}

fn physical_length(logical: f64, scale: f64) -> u32 {
    (logical * scale).round().max(1.0) as u32
}

fn side_for(bounds: Bounds, work_area: Bounds) -> FloatingBubbleSide {
    let center = bounds.x as i64 + bounds.width as i64 / 2;
    let area_center = work_area.x as i64 + work_area.width as i64 / 2;
    if center <= area_center {
        FloatingBubbleSide::Left
    } else {
        FloatingBubbleSide::Right
    }
}

fn clamp_i32(value: i64, min: i64, max: i64) -> i32 {
    value.clamp(min, max.max(min)) as i32
}

fn collapsed_bounds(
    expanded: Bounds,
    work_area: Bounds,
    bubble_size: u32,
    y_margin: i32,
) -> Bounds {
    let side = side_for(expanded, work_area);
    let width = bubble_size;
    let height = bubble_size;
    let min_y = work_area.y as i64 + y_margin as i64;
    let max_y = work_area.y as i64 + work_area.height as i64 - height as i64 - y_margin as i64;
    let desired_y = expanded.y as i64 + (expanded.height as i64 - height as i64) / 2;
    let x = match side {
        FloatingBubbleSide::Left => work_area.x,
        FloatingBubbleSide::Right => work_area.x + work_area.width as i32 - width as i32,
    };
    Bounds {
        x,
        y: clamp_i32(desired_y, min_y, max_y),
        width,
        height,
    }
}
fn dragged_bounds(
    cursor: (f64, f64),
    area: Bounds,
    bubble_size: u32,
    offset_ratio: (f64, f64),
    y_margin: i32,
) -> Bounds {
    let ratio_x = offset_ratio.0.clamp(0.0, 1.0);
    let ratio_y = offset_ratio.1.clamp(0.0, 1.0);
    let desired_x = cursor.0 - ratio_x * bubble_size as f64;
    let desired_y = cursor.1 - ratio_y * bubble_size as f64;
    let min_x = area.x as i64;
    let max_x = area.x as i64 + area.width as i64 - bubble_size as i64;
    let min_y = area.y as i64 + y_margin as i64;
    let max_y = area.y as i64 + area.height as i64 - bubble_size as i64 - y_margin as i64;
    Bounds {
        x: clamp_i32(desired_x.round() as i64, min_x, max_x),
        y: clamp_i32(desired_y.round() as i64, min_y, max_y),
        width: bubble_size,
        height: bubble_size,
    }
}

fn expanded_bounds(
    collapsed: Bounds,
    work_area: Bounds,
    logical_size: (f64, f64),
    scale: f64,
) -> Bounds {
    let width = physical_length(logical_size.0, scale);
    let height = physical_length(logical_size.1, scale);
    let side = side_for(collapsed, work_area);
    let min_x = work_area.x as i64 + EXPANDED_MARGIN as i64;
    let max_x = work_area.x as i64 + work_area.width as i64 - width as i64 - EXPANDED_MARGIN as i64;
    let min_y = work_area.y as i64 + EXPANDED_MARGIN as i64;
    let max_y =
        work_area.y as i64 + work_area.height as i64 - height as i64 - EXPANDED_MARGIN as i64;
    let desired_x = match side {
        FloatingBubbleSide::Left => collapsed.x as i64,
        FloatingBubbleSide::Right => collapsed.x as i64 + collapsed.width as i64 - width as i64,
    };
    let desired_y = collapsed.y as i64 + (collapsed.height as i64 - height as i64) / 2;
    Bounds {
        x: clamp_i32(desired_x, min_x, max_x),
        y: clamp_i32(desired_y, min_y, max_y),
        width,
        height,
    }
}
fn payload(settings: &SettingsStore, state: &BubbleState) -> Result<FloatingBubblePayload, String> {
    Ok(FloatingBubblePayload {
        enabled: settings.get()?.floating_bubble_enabled,
        collapsed: state.collapsed,
        side: state.side,
    })
}

pub fn current_state(
    settings: &SettingsStore,
    controller: &FloatingBubbleController,
) -> Result<FloatingBubblePayload, String> {
    current_payload(settings, controller)
}

pub fn collapse(
    window: &WebviewWindow,
    settings: &SettingsStore,
    controller: &FloatingBubbleController,
) -> Result<FloatingBubblePayload, String> {
    if !settings.get()?.floating_bubble_enabled {
        let state = controller
            .state
            .lock()
            .map_err(|_| "floating bubble state lock was poisoned".to_owned())?;
        return payload(settings, &state);
    }
    if window.is_focused().map_err(window_error)? {
        let state = controller
            .state
            .lock()
            .map_err(|_| "floating bubble state lock was poisoned".to_owned())?;
        return payload(settings, &state);
    }
    let mut state = controller
        .state
        .lock()
        .map_err(|_| "floating bubble state lock was poisoned".to_owned())?;
    if state.collapsed {
        return payload(settings, &state);
    }
    let current = Bounds::from_window(window)?;
    let monitor = window
        .current_monitor()
        .map_err(window_error)?
        .ok_or_else(|| "no monitor is available for the floating bubble".to_owned())?;
    let scale = monitor.scale_factor();
    let policy = PlatformPolicy::current();
    let area = collapsed_area(&monitor, policy);
    let target = collapsed_bounds(
        current,
        area,
        physical_length(BUBBLE_LOGICAL_SIZE, scale),
        policy.collapsed_y_margin(),
    );
    state.expanded = Some(ExpandedState {
        logical_width: current.width as f64 / scale,
        logical_height: current.height as f64 / scale,
        always_on_top: window.is_always_on_top().map_err(window_error)?,
    });
    state.collapsed = true;
    state.side = Some(side_for(target, area));
    apply_collapsed_window(window, target)?;
    appearance::apply_backdrop(window, &settings.get()?, true);
    payload(settings, &state)
}
fn apply_collapsed_window(window: &WebviewWindow, target: Bounds) -> Result<(), String> {
    let bubble_size = PhysicalSize::new(target.width, target.height);
    window
        .set_min_size(Some(bubble_size))
        .map_err(window_error)?;
    window
        .set_max_size(Some(bubble_size))
        .map_err(window_error)?;
    window.set_resizable(false).map_err(window_error)?;
    window.set_decorations(false).map_err(window_error)?;
    window.set_shadow(false).map_err(window_error)?;
    window.set_always_on_top(true).map_err(window_error)?;
    let _ = window.set_skip_taskbar(true);
    window.set_size(bubble_size).map_err(window_error)?;
    window
        .set_position(PhysicalPosition::new(target.x, target.y))
        .map_err(window_error)?;
    Ok(())
}

fn restore_size_constraints(window: &WebviewWindow) -> Result<(), String> {
    window
        .set_min_size(Some(LogicalSize::new(
            EXPANDED_MIN_WIDTH,
            EXPANDED_MIN_HEIGHT,
        )))
        .map_err(window_error)?;
    window
        .set_max_size::<PhysicalSize<u32>>(None)
        .map_err(window_error)?;
    window.set_resizable(true).map_err(window_error)?;
    Ok(())
}
pub fn expand(
    window: &WebviewWindow,
    settings: &SettingsStore,
    controller: &FloatingBubbleController,
    focus: bool,
) -> Result<FloatingBubblePayload, String> {
    let mut state = controller
        .state
        .lock()
        .map_err(|_| "floating bubble state lock was poisoned".to_owned())?;
    if !state.collapsed {
        return payload(settings, &state);
    }
    let expanded = state
        .expanded
        .ok_or_else(|| "floating bubble has no expanded bounds to restore".to_owned())?;
    let current = Bounds::from_window(window)?;
    let center_x = current.x as f64 + current.width as f64 / 2.0;
    let center_y = current.y as f64 + current.height as f64 / 2.0;
    let monitor = window
        .monitor_from_point(center_x, center_y)
        .map_err(window_error)?
        .or_else(|| window.current_monitor().ok().flatten())
        .ok_or_else(|| "no monitor is available for floating bubble expansion".to_owned())?;
    let target = expanded_bounds(
        current,
        monitor_work_area(&monitor),
        (expanded.logical_width, expanded.logical_height),
        monitor.scale_factor(),
    );
    restore_size_constraints(window)?;
    window
        .set_size(LogicalSize::new(
            expanded.logical_width,
            expanded.logical_height,
        ))
        .map_err(window_error)?;
    window
        .set_position(PhysicalPosition::new(target.x, target.y))
        .map_err(window_error)?;
    window
        .set_always_on_top(expanded.always_on_top)
        .map_err(window_error)?;
    let _ = window.set_skip_taskbar(false);
    window.show().map_err(window_error)?;
    if focus {
        window.set_focus().map_err(window_error)?;
    }
    state.collapsed = false;
    state.side = None;
    appearance::apply_backdrop(window, &settings.get()?, false);
    payload(settings, &state)
}

pub fn expand_if_disabled(
    window: &WebviewWindow,
    settings: &SettingsStore,
    controller: &FloatingBubbleController,
) -> Result<FloatingBubblePayload, String> {
    if settings.get()?.floating_bubble_enabled {
        return current_payload(settings, controller);
    }
    expand(window, settings, controller, false)
}
fn current_payload(
    settings: &SettingsStore,
    controller: &FloatingBubbleController,
) -> Result<FloatingBubblePayload, String> {
    let state = controller
        .state
        .lock()
        .map_err(|_| "floating bubble state lock was poisoned".to_owned())?;
    payload(settings, &state)
}

pub fn move_to_cursor(
    window: &WebviewWindow,
    settings: &SettingsStore,
    controller: &FloatingBubbleController,
    offset: BubbleDragOffset,
) -> Result<FloatingBubblePayload, String> {
    let mut state = controller
        .state
        .lock()
        .map_err(|_| "floating bubble state lock was poisoned".to_owned())?;
    if !state.collapsed {
        return payload(settings, &state);
    }
    let cursor = window.cursor_position().map_err(window_error)?;
    let monitor = window
        .monitor_from_point(cursor.x, cursor.y)
        .map_err(window_error)?
        .or_else(|| window.current_monitor().ok().flatten())
        .ok_or_else(|| "no monitor is available while moving the floating bubble".to_owned())?;
    let policy = PlatformPolicy::current();
    let area = collapsed_area(&monitor, policy);
    let bubble_size = physical_length(BUBBLE_LOGICAL_SIZE, monitor.scale_factor());
    let target = dragged_bounds(
        (cursor.x, cursor.y),
        area,
        bubble_size,
        (
            offset.offset_ratio_x.unwrap_or(0.5),
            offset.offset_ratio_y.unwrap_or(0.5),
        ),
        policy.collapsed_y_margin(),
    );
    let target_size = PhysicalSize::new(target.width, target.height);
    window
        .set_min_size(Some(target_size))
        .map_err(window_error)?;
    window
        .set_max_size(Some(target_size))
        .map_err(window_error)?;
    window.set_size(target_size).map_err(window_error)?;
    window
        .set_position(PhysicalPosition::new(target.x, target.y))
        .map_err(window_error)?;
    state.side = Some(side_for(target, area));
    payload(settings, &state)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn area() -> Bounds {
        Bounds {
            x: 0,
            y: 24,
            width: 1440,
            height: 876,
        }
    }
    #[test]
    fn collapse_docks_to_nearest_edge_and_centers_vertically() {
        let expanded = Bounds {
            x: 1000,
            y: 200,
            width: 340,
            height: 650,
        };
        let result = collapsed_bounds(expanded, area(), 34, COLLAPSED_Y_MARGIN);
        assert_eq!(result.x, 1406);
        assert_eq!(result.y, 508);
        assert_eq!(result.width, 34);
        assert_eq!(side_for(result, area()), FloatingBubbleSide::Right);
    }

    #[test]
    fn collapse_clamps_vertical_position_to_work_area_margin() {
        let expanded = Bounds {
            x: 10,
            y: -500,
            width: 340,
            height: 650,
        };
        let result = collapsed_bounds(expanded, area(), 34, COLLAPSED_Y_MARGIN);
        assert_eq!(result.x, 0);
        assert_eq!(result.y, 32);
        assert_eq!(side_for(result, area()), FloatingBubbleSide::Left);
    }

    #[test]
    fn windows_collapse_uses_full_display_and_zero_vertical_margin() {
        let full = Bounds {
            x: 0,
            y: 0,
            width: 1440,
            height: 900,
        };
        let work = area();
        let expanded = Bounds {
            x: 1000,
            y: 700,
            width: 340,
            height: 650,
        };
        let selected = select_collapsed_area(full, work, PlatformPolicy::Windows);
        let result = collapsed_bounds(
            expanded,
            selected,
            34,
            PlatformPolicy::Windows.collapsed_y_margin(),
        );
        assert_eq!(selected, full);
        assert_eq!(result.x, 1406);
        assert_eq!(result.y, 866);
    }

    #[test]
    fn desktop_collapse_uses_work_area_and_eight_pixel_vertical_margin() {
        let full = Bounds {
            x: 0,
            y: 0,
            width: 1440,
            height: 900,
        };
        let work = area();
        let expanded = Bounds {
            x: 1000,
            y: 700,
            width: 340,
            height: 650,
        };
        let selected = select_collapsed_area(full, work, PlatformPolicy::Desktop);
        let result = collapsed_bounds(
            expanded,
            selected,
            34,
            PlatformPolicy::Desktop.collapsed_y_margin(),
        );
        assert_eq!(selected, work);
        assert_eq!(result.y, 858);
    }

    #[test]
    fn bubble_size_tracks_fractional_windows_scale_factors() {
        assert_eq!(physical_length(BUBBLE_LOGICAL_SIZE, 1.0), 34);
        assert_eq!(physical_length(BUBBLE_LOGICAL_SIZE, 1.25), 43);
        assert_eq!(physical_length(BUBBLE_LOGICAL_SIZE, 1.5), 51);
    }

    #[test]
    fn dragging_recomputes_physical_bubble_size_for_destination_dpi() {
        let full = Bounds {
            x: 1440,
            y: 0,
            width: 1920,
            height: 1080,
        };
        let bubble_size = physical_length(BUBBLE_LOGICAL_SIZE, 1.5);
        let result = dragged_bounds(
            (3350.0, 1000.0),
            full,
            bubble_size,
            (0.5, 0.5),
            PlatformPolicy::Windows.collapsed_y_margin(),
        );
        assert_eq!(bubble_size, 51);
        assert_eq!(result.width, 51);
        assert_eq!(result.height, 51);
        assert!(result.x <= 3309);
        assert!(result.y <= 1029);
    }

    #[test]
    fn expansion_keeps_the_collapsed_edge_and_restores_size() {
        let collapsed = Bounds {
            x: 1406,
            y: 508,
            width: 34,
            height: 34,
        };
        let result = expanded_bounds(collapsed, area(), (340.0, 650.0), 1.0);
        assert_eq!(result.width, 340);
        assert_eq!(result.height, 650);
        assert_eq!(result.x, 1092);
        assert_eq!(result.y, 200);
    }
}
