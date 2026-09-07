use crate::appearance;
use crate::settings::SettingsStore;
use crate::window_state;
use serde::Serialize;
use std::sync::Mutex;
use tauri::{LogicalSize, Monitor, PhysicalPosition, PhysicalSize, WebviewWindow};

const BUBBLE_LOGICAL_HEIGHT: f64 = 34.0;
const BUBBLE_LOGICAL_MIN_WIDTH: f64 = 34.0;
const BUBBLE_LOGICAL_MAX_WIDTH: f64 = 240.0;
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

#[derive(Debug)]
struct BubbleState {
    collapsed: bool,
    side: Option<FloatingBubbleSide>,
    expanded: Option<ExpandedState>,
    collapsed_logical_width: f64,
}

impl Default for BubbleState {
    fn default() -> Self {
        Self {
            collapsed: false,
            side: None,
            expanded: None,
            collapsed_logical_width: BUBBLE_LOGICAL_MIN_WIDTH,
        }
    }
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

fn normalized_logical_width(width: f64, bubble_scale: f64) -> f64 {
    let bubble_scale = if bubble_scale.is_finite() {
        bubble_scale.clamp(0.7, 1.5)
    } else {
        1.0
    };
    let min_width = BUBBLE_LOGICAL_MIN_WIDTH * bubble_scale;
    let max_width = BUBBLE_LOGICAL_MAX_WIDTH * bubble_scale;
    if !width.is_finite() {
        return min_width;
    }
    width.clamp(min_width, max_width)
}

fn physical_bubble_size(logical_width: f64, bubble_scale: f64, monitor_scale: f64) -> (u32, u32) {
    (
        physical_length(
            normalized_logical_width(logical_width, bubble_scale),
            monitor_scale,
        ),
        physical_length(BUBBLE_LOGICAL_HEIGHT * bubble_scale, monitor_scale),
    )
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
    bubble_width: u32,
    bubble_height: u32,
    y_margin: i32,
) -> Bounds {
    let side = side_for(expanded, work_area);
    let width = bubble_width;
    let height = bubble_height;
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
    bubble_width: u32,
    bubble_height: u32,
    offset_ratio: (f64, f64),
    y_margin: i32,
) -> Bounds {
    let ratio_x = offset_ratio.0.clamp(0.0, 1.0);
    let ratio_y = offset_ratio.1.clamp(0.0, 1.0);
    let desired_x = cursor.0 - ratio_x * bubble_width as f64;
    let desired_y = cursor.1 - ratio_y * bubble_height as f64;
    let min_x = area.x as i64;
    let max_x = area.x as i64 + area.width as i64 - bubble_width as i64;
    let min_y = area.y as i64 + y_margin as i64;
    let max_y = area.y as i64 + area.height as i64 - bubble_height as i64 - y_margin as i64;
    Bounds {
        x: clamp_i32(desired_x.round() as i64, min_x, max_x),
        y: clamp_i32(desired_y.round() as i64, min_y, max_y),
        width: bubble_width,
        height: bubble_height,
    }
}

fn resized_collapsed_bounds(
    current: Bounds,
    area: Bounds,
    bubble_width: u32,
    bubble_height: u32,
    side: FloatingBubbleSide,
    y_margin: i32,
) -> Bounds {
    let min_x = area.x as i64;
    let max_x = area.x as i64 + area.width as i64 - bubble_width as i64;
    let min_y = area.y as i64 + y_margin as i64;
    let max_y = area.y as i64 + area.height as i64 - bubble_height as i64 - y_margin as i64;
    let center_y = current.y as i64 + current.height as i64 / 2;
    let desired_x = match side {
        FloatingBubbleSide::Left => current.x as i64,
        FloatingBubbleSide::Right => current.x as i64 + current.width as i64 - bubble_width as i64,
    };
    Bounds {
        x: clamp_i32(desired_x, min_x, max_x),
        y: clamp_i32(center_y - bubble_height as i64 / 2, min_y, max_y),
        width: bubble_width,
        height: bubble_height,
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

fn collapse_impl(
    window: &WebviewWindow,
    settings: &SettingsStore,
    controller: &FloatingBubbleController,
    require_unfocused: bool,
) -> Result<FloatingBubblePayload, String> {
    if !settings.get()?.floating_bubble_enabled
        || (require_unfocused && window.is_focused().map_err(window_error)?)
    {
        return current_payload(settings, controller);
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
    let bubble_scale = settings.get()?.floating_bubble_scale;
    let policy = PlatformPolicy::current();
    let area = collapsed_area(&monitor, policy);
    let (bubble_width, bubble_height) =
        physical_bubble_size(state.collapsed_logical_width, bubble_scale, scale);
    let target = collapsed_bounds(
        current,
        area,
        bubble_width,
        bubble_height,
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

pub fn collapse(
    window: &WebviewWindow,
    settings: &SettingsStore,
    controller: &FloatingBubbleController,
) -> Result<FloatingBubblePayload, String> {
    collapse_impl(window, settings, controller, false)
}

pub fn collapse_if_idle(
    window: &WebviewWindow,
    settings: &SettingsStore,
    controller: &FloatingBubbleController,
) -> Result<FloatingBubblePayload, String> {
    collapse_impl(window, settings, controller, true)
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
    window_state::restore_expanded_constraints(window)
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

pub fn set_collapsed_width(
    window: &WebviewWindow,
    settings: &SettingsStore,
    controller: &FloatingBubbleController,
    width: f64,
) -> Result<FloatingBubblePayload, String> {
    let bubble_scale = settings.get()?.floating_bubble_scale;
    let mut state = controller
        .state
        .lock()
        .map_err(|_| "floating bubble state lock was poisoned".to_owned())?;
    state.collapsed_logical_width = normalized_logical_width(width, bubble_scale);
    if !state.collapsed {
        return payload(settings, &state);
    }
    let current = Bounds::from_window(window)?;
    let center_x = current.x as f64 + current.width as f64 / 2.0;
    let center_y = current.y as f64 + current.height as f64 / 2.0;
    let monitor = window
        .monitor_from_point(center_x, center_y)
        .map_err(window_error)?
        .or_else(|| window.current_monitor().ok().flatten())
        .ok_or_else(|| "no monitor is available while resizing the floating bubble".to_owned())?;
    let policy = PlatformPolicy::current();
    let area = collapsed_area(&monitor, policy);
    let (bubble_width, bubble_height) = physical_bubble_size(
        state.collapsed_logical_width,
        bubble_scale,
        monitor.scale_factor(),
    );
    let side = state.side.unwrap_or_else(|| side_for(current, area));
    let target = resized_collapsed_bounds(
        current,
        area,
        bubble_width,
        bubble_height,
        side,
        policy.collapsed_y_margin(),
    );
    apply_collapsed_window(window, target)?;
    state.side = Some(side);
    payload(settings, &state)
}

pub fn move_to_cursor(
    window: &WebviewWindow,
    settings: &SettingsStore,
    controller: &FloatingBubbleController,
    offset: BubbleDragOffset,
) -> Result<FloatingBubblePayload, String> {
    let bubble_scale = settings.get()?.floating_bubble_scale;
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
    let (bubble_width, bubble_height) = physical_bubble_size(
        state.collapsed_logical_width,
        bubble_scale,
        monitor.scale_factor(),
    );
    let target = dragged_bounds(
        (cursor.x, cursor.y),
        area,
        bubble_width,
        bubble_height,
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
        let result = collapsed_bounds(expanded, area(), 34, 34, COLLAPSED_Y_MARGIN);
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
        let result = collapsed_bounds(expanded, area(), 34, 34, COLLAPSED_Y_MARGIN);
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
            34,
            PlatformPolicy::Desktop.collapsed_y_margin(),
        );
        assert_eq!(selected, work);
        assert_eq!(result.y, 858);
    }

    #[test]
    fn bubble_size_tracks_user_scale_and_fractional_monitor_dpi() {
        assert_eq!(physical_bubble_size(34.0, 1.0, 1.0), (34, 34));
        assert_eq!(physical_bubble_size(96.0, 1.0, 1.25), (120, 43));
        assert_eq!(physical_bubble_size(115.2, 1.2, 1.25), (144, 51));
    }

    #[test]
    fn dragging_recomputes_variable_width_for_destination_dpi() {
        let full = Bounds {
            x: 1440,
            y: 0,
            width: 1920,
            height: 1080,
        };
        let (bubble_width, bubble_height) = physical_bubble_size(96.0, 1.0, 1.5);
        let result = dragged_bounds(
            (3350.0, 1000.0),
            full,
            bubble_width,
            bubble_height,
            (0.5, 0.5),
            PlatformPolicy::Windows.collapsed_y_margin(),
        );
        assert_eq!(result.width, 144);
        assert_eq!(result.height, 51);
        assert!(result.x <= 3216);
        assert!(result.y <= 1029);
    }

    #[test]
    fn collapsed_resize_preserves_the_users_edge_gap() {
        assert_eq!(normalized_logical_width(10.0, 1.0), 34.0);
        assert_eq!(normalized_logical_width(500.0, 1.0), 240.0);
        assert_eq!(normalized_logical_width(10.0, 1.5), 51.0);
        assert_eq!(normalized_logical_width(500.0, 1.5), 360.0);
        let current = Bounds {
            x: 1386,
            y: 508,
            width: 34,
            height: 34,
        };
        let resized = resized_collapsed_bounds(
            current,
            area(),
            120,
            34,
            FloatingBubbleSide::Right,
            COLLAPSED_Y_MARGIN,
        );
        assert_eq!(resized.x, 1300);
        assert_eq!(resized.y, 508);
        assert_eq!(resized.width, 120);
        assert_eq!(resized.height, 34);
        assert_eq!(1440 - (resized.x + resized.width as i32), 20);
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
