//! Host-owned panel-kit workspace composition shared by every route.
//!
//! The app owns reducer state, persistence timing, event priority, projection
//! scratch, and render order. Panel-kit supplies the pure core and web painters.

use std::cell::RefCell;
use std::rc::Rc;

use crate::components::footer::FooterMeta;
use dioxus::events::{KeyboardEvent, PointerEvent as DioxusPointerEvent, WheelEvent};
use dioxus::prelude::*;
use panel_kit::input::{
    clear_selection, keyboard_event, pointer_event, release_pointer, tile_drag_pointer_event,
    use_tile_drag_frames, wheel_event,
};
use panel_kit::store::LocalStorageLayoutStore;
use panel_kit::surface::{observe_viewport, surface_profile, viewport_size};
use panel_kit::widgets::{dock, panel, root};
use panel_kit::{
    Clamp, CommandStep, FocusContext, LayoutBuilder, Mode, PanelCommand, PanelKind, PanelWin,
    PointerButton, PointerEventKind, SnapPolicy, SurfaceClass, Units, WinState,
};
use panel_kit_core::frame::{
    project_into, ChromeProjectionInput, Placement, ProjectedFrame, ProjectionBuffer,
    ProjectionInput, TileLayoutMetrics,
};
use panel_kit_core::persist::{
    apply_save_decision, restore_snapshot, LayoutError, RestoreContext, SavePolicy,
};
use panel_kit_core::reducer::{
    reduce, HitTarget, ReduceContext, ResizePolicy, Snapshot, Viewport, WorkspaceEvent,
};
use panel_kit_core::{ChromeMetrics, PanelCatalog, TileMetrics};

/// Host-owned state and ports for one route workspace.
#[derive(Clone)]
pub(crate) struct PanelWorkspace<K: PanelKind> {
    pub(crate) storage_key: &'static str,
    pub(crate) snapshot: Signal<Snapshot<K>>,
    pub(crate) catalog: Rc<PanelCatalog<K>>,
    scratch: Rc<RefCell<ProjectionBuffer<K>>>,
    store: Rc<LocalStorageLayoutStore>,
    save_policy: SavePolicy,
}

impl<K: PanelKind> PartialEq for PanelWorkspace<K> {
    fn eq(&self, other: &Self) -> bool {
        self.storage_key == other.storage_key
            && self.snapshot == other.snapshot
            && Rc::ptr_eq(&self.catalog, &other.catalog)
            && Rc::ptr_eq(&self.scratch, &other.scratch)
            && Rc::ptr_eq(&self.store, &other.store)
    }
}

/// Restore one route's stable layout key and mount its viewport observer.
pub(crate) fn use_panel_workspace<K: PanelKind>(
    storage_key: &'static str,
    defaults: fn() -> Vec<PanelWin<K>>,
) -> PanelWorkspace<K> {
    let initial_viewport = current_viewport();
    let default_snapshot = Snapshot::from_defaults(defaults(), Mode::Tiling, initial_viewport);
    let catalog_panels = default_snapshot.panels.clone();
    let catalog = use_hook(move || {
        Rc::new(
            PanelCatalog::from_panel_kind_layout(&catalog_panels)
                .expect("workspace panel enum must serialize as stable string IDs"),
        )
    });
    let store = use_hook(move || Rc::new(LocalStorageLayoutStore::new(storage_key)));
    let snapshot = use_signal({
        let catalog = catalog.clone();
        let store = store.clone();
        let defaults = default_snapshot.clone();
        move || restore_or_default(storage_key, &store, defaults, &catalog)
    });
    let scratch = use_hook({
        let panel_count = snapshot.peek().panels.len();
        move || {
            Rc::new(RefCell::new(ProjectionBuffer::with_panel_capacity(
                panel_count,
            )))
        }
    });

    let workspace = PanelWorkspace {
        storage_key,
        snapshot,
        catalog,
        scratch,
        store,
        save_policy: SavePolicy::OnSettle,
    };
    mount_viewport_observer(&workspace);
    // Core reads no clock: while a tile drag animates, one animation frame at a
    // time is requested and reaches the reducer as `WorkspaceEvent::Tick`.
    let animating = workspace
        .snapshot
        .read()
        .tile_drag
        .as_ref()
        .is_some_and(|tile| tile.animating());
    use_tile_drag_frames(animating, workspace_event_handler(&workspace));
    workspace
}

/// Paint a complete route workspace from explicit v1 panel parts.
///
/// `restore_shortcuts` is app-first input policy (used by the admin route).
/// All other keyboard input is translated by panel-kit's input adapter.
pub(crate) fn render_workspace<K, F>(
    workspace: &PanelWorkspace<K>,
    mut body: F,
    restore_shortcuts: &'static [(&'static str, K)],
) -> Element
where
    K: PanelKind,
    F: FnMut(K, bool) -> Element + 'static,
{
    let emit = workspace_event_handler(workspace);
    let pointer_move_workspace = workspace.clone();
    let pointer_up_workspace = workspace.clone();
    let pointer_cancel_workspace = workspace.clone();
    let key_workspace = workspace.clone();
    let wheel_workspace = workspace.clone();

    let snapshot = workspace.snapshot.read();
    let mut scratch = workspace.scratch.borrow_mut();
    let frame = project_workspace(&snapshot, &mut scratch);
    let root_class = root::root_class(&frame);
    let workspace_class = workspace_area_class(&frame);
    let workspace_style = frame
        .tile_grid
        .map(root::tile_grid_style)
        .unwrap_or_default();

    rsx! {
        div {
            class: "{root_class}",
            tabindex: "0",
            onpointermove: move |event: DioxusPointerEvent| {
                handle_pointer_move(&pointer_move_workspace, &event)
            },
            onpointerup: move |event: DioxusPointerEvent| {
                handle_pointer_up(&pointer_up_workspace, &event)
            },
            onpointercancel: move |event: DioxusPointerEvent| {
                handle_pointer_cancel(&pointer_cancel_workspace, &event)
            },
            onkeydown: move |event: KeyboardEvent| {
                if !panel_kit::input::is_editing() {
                    if let Key::Character(character) = event.key() {
                        if let Some((_, target)) = restore_shortcuts
                            .iter()
                            .find(|(shortcut, _)| *shortcut == character.as_str())
                        {
                            event.prevent_default();
                            restore_panel(&key_workspace, *target);
                            return;
                        }
                    }
                }
                handle_key(&key_workspace, &event);
            },
            div {
                class: "{workspace_class}",
                style: "{workspace_style}",
                onwheel: move |event: WheelEvent| handle_wheel(&wheel_workspace, &event),
                for projected in frame.panels.iter().copied() {
                    {
                        let maximized = matches!(projected.placement, Placement::Maximized);
                        render_projected_panel(
                            projected,
                            &workspace.catalog,
                            emit,
                            body(projected.key, maximized),
                        )
                    }
                }
            }
            {dock::dock(frame.dock, &workspace.catalog, emit, Some(rsx! { FooterMeta {} }))}
        }
    }
}

/// Read the current surface tier from host-owned viewport state.
pub(crate) fn surface_class<K: PanelKind>(workspace: &PanelWorkspace<K>) -> SurfaceClass {
    surface_profile(workspace.snapshot.read().viewport.width).class
}

/// Restore one panel through the reducer and explicit persistence policy.
pub(crate) fn restore_panel<K: PanelKind>(workspace: &PanelWorkspace<K>, target: K) {
    workspace_event_handler(workspace).call(WorkspaceEvent::Command {
        target: Some(target),
        command: PanelCommand::Restore,
    });
}

/// Set the preferred layout mode through the same reducer path as the blue light.
pub(crate) fn set_mode<K: PanelKind>(workspace: &PanelWorkspace<K>, mode: Mode) {
    if workspace.snapshot.read().preferred_mode != mode {
        workspace_event_handler(workspace).call(WorkspaceEvent::Command {
            target: None,
            command: PanelCommand::ToggleMode,
        });
    }
}

fn render_projected_panel<K: PanelKind>(
    projected: panel_kit_core::frame::PanelProjection<K>,
    catalog: &PanelCatalog<K>,
    emit: EventHandler<WorkspaceEvent<K>>,
    body: Element,
) -> Element {
    let Some(meta) = catalog.get(projected.key) else {
        return rsx! {};
    };
    let class = format!("panel-{}", meta.slug);
    let controls = panel::traffic_lights(projected, emit);
    let chrome = panel::panel_chrome_with_events(projected, meta, emit, Some(controls), None);
    let body = panel::panel_body(body);
    let resize = panel::resize_grip(projected, emit);

    panel::panel_shell_with_events(
        projected,
        Some(&class),
        rsx! { {chrome} {body} {resize} },
        emit,
    )
}

fn mount_viewport_observer<K: PanelKind>(workspace: &PanelWorkspace<K>) {
    let emit = workspace_event_handler(workspace);
    let _status = observe_viewport(EventHandler::new(move |size: Viewport| {
        emit.call(WorkspaceEvent::ViewportChanged {
            size,
            policy: ResizePolicy::ScaleFloating,
        });
    }));
}

fn workspace_event_handler<K: PanelKind>(
    workspace: &PanelWorkspace<K>,
) -> EventHandler<WorkspaceEvent<K>> {
    let workspace = workspace.clone();
    EventHandler::new(move |event| {
        reduce_and_persist(&workspace, event);
    })
}

fn reduce_and_persist<K: PanelKind>(
    workspace: &PanelWorkspace<K>,
    event: WorkspaceEvent<K>,
) -> bool {
    let mut snapshot_signal = workspace.snapshot;
    let mut snapshot = snapshot_signal.write();
    let context = reduce_context(&snapshot);
    let reduction = reduce(&mut snapshot, event, context);
    let changed = reduction.changed;
    let decision = workspace.save_policy.decide(&reduction);

    if let Err(error) =
        apply_save_decision(decision, &*workspace.store, &snapshot, &workspace.catalog)
    {
        log_layout_error("save layout", workspace.storage_key, &error);
    }

    changed
}

fn handle_key<K: PanelKind>(workspace: &PanelWorkspace<K>, event: &KeyboardEvent) {
    let focus = if panel_kit::input::is_editing() {
        FocusContext::TextInput
    } else if let Some(key) = workspace.snapshot.read().focused {
        FocusContext::Panel(key)
    } else {
        FocusContext::Workspace
    };

    let Some(workspace_event) = keyboard_event(event, focus) else {
        return;
    };
    if reduce_and_persist(workspace, workspace_event) {
        event.prevent_default();
    }
}

fn handle_pointer_move<K: PanelKind>(workspace: &PanelWorkspace<K>, event: &DioxusPointerEvent) {
    let workspace_event = if workspace.snapshot.read().tile_drag.is_some() {
        // A held or lifted tile resolves its target against the painted slots.
        let snapshot = workspace.snapshot.read();
        let mut scratch = workspace.scratch.borrow_mut();
        let frame = project_workspace(&snapshot, &mut scratch);
        let tile_event = tile_drag_pointer_event(&frame, event);
        tile_event
    } else {
        let kind = if workspace.snapshot.read().drag.is_some() {
            PointerEventKind::Drag(PointerButton::Primary)
        } else {
            PointerEventKind::Moved
        };
        pointer_event(HitTarget::Workspace, event, kind)
    };
    reduce_and_persist(workspace, workspace_event);
}

fn handle_pointer_up<K: PanelKind>(workspace: &PanelWorkspace<K>, event: &DioxusPointerEvent) {
    finish_pointer(
        workspace,
        event,
        PointerEventKind::Up(PointerButton::Primary),
    );
}

/// A cancelled pointer must never commit a drag, so it maps to `Cancel`, not `Up`.
fn handle_pointer_cancel<K: PanelKind>(workspace: &PanelWorkspace<K>, event: &DioxusPointerEvent) {
    finish_pointer(workspace, event, PointerEventKind::Cancel);
}

fn finish_pointer<K: PanelKind>(
    workspace: &PanelWorkspace<K>,
    event: &DioxusPointerEvent,
    kind: PointerEventKind,
) {
    release_pointer(event);
    let changed = reduce_and_persist(workspace, pointer_event(HitTarget::Workspace, event, kind));
    if changed {
        clear_selection();
    }
}

fn handle_wheel<K: PanelKind>(workspace: &PanelWorkspace<K>, event: &WheelEvent) {
    if reduce_and_persist(workspace, wheel_event(event)) {
        event.prevent_default();
    }
}

fn project_workspace<'frame, K: PanelKind>(
    snapshot: &Snapshot<K>,
    scratch: &'frame mut ProjectionBuffer<K>,
) -> ProjectedFrame<'frame, K> {
    let surface = surface_profile(snapshot.viewport.width);
    let chrome = ChromeProjectionInput::full(ChromeMetrics::WEB);
    let tile = TileLayoutMetrics::from_tile_metrics(TileMetrics::WEB, surface);

    project_into(
        ProjectionInput {
            snapshot,
            surface,
            chrome: &chrome,
            clamp: &Clamp::WEB,
            tile: &tile,
        },
        scratch,
    )
}

fn workspace_area_class<K: PanelKind>(frame: &ProjectedFrame<'_, K>) -> &'static str {
    if frame
        .panels
        .iter()
        .any(|panel| matches!(panel.placement, Placement::Maximized))
    {
        "ws maxed"
    } else if frame.mode == Mode::Tiling {
        "ws tiling"
    } else {
        "ws floating"
    }
}

fn reduce_context<K: PanelKind>(snapshot: &Snapshot<K>) -> ReduceContext<'static> {
    ReduceContext {
        surface: surface_profile(snapshot.viewport.width),
        clamp: &Clamp::WEB,
        command_step: CommandStep::WEB,
        tile: &TileMetrics::WEB,
        snap: SnapPolicy::default(),
    }
}

pub(crate) fn current_viewport() -> Viewport {
    let (width, height) = viewport_size();
    Viewport {
        width,
        height,
        units: Units::CssPx,
    }
}

/// Whether this viewport is the Compact surface tier (phones): panels tile
/// without window-management chrome, so one panel fills the workspace and the
/// rest wait in the dock.
pub(crate) fn is_compact(vp: Viewport) -> bool {
    surface_profile(vp.width).class == SurfaceClass::Compact
}

/// [`is_compact`] for the live viewport.
pub(crate) fn is_compact_now() -> bool {
    is_compact(current_viewport())
}

/// Defaults for a two-region page. Regular/Tablet: `primary_share` of the
/// width to the primary, the rest to the secondary. Compact: the primary fills
/// the workspace and the secondary waits in the dock.
pub(crate) fn split_defaults<K: PanelKind>(
    primary: K,
    secondary: K,
    primary_share: f64,
    margin: f64,
    gap: f64,
) -> Vec<PanelWin<K>> {
    let vp = current_viewport();
    let mut b = LayoutBuilder::new();
    if is_compact(vp) {
        return compact_pair(&mut b, primary, secondary, vp, margin);
    }
    let inner_w = vp.width - 2.0 * margin - gap;
    let left_w = (inner_w * primary_share).max(280.0);
    let right_w = (inner_w - left_w).max(240.0);
    let h = (vp.height - 2.0 * margin).max(360.0);
    vec![
        b.at(primary, margin, margin, left_w, h),
        b.at(secondary, margin + left_w + gap, margin, right_w, h),
    ]
}

/// Defaults for a row-major grid of equal regions (Regular/Tablet) that docks
/// down to the primary on Compact.
pub(crate) fn grid_defaults<K: PanelKind>(
    panels: &[K],
    columns: usize,
    margin: f64,
    gap: f64,
) -> Vec<PanelWin<K>> {
    let vp = current_viewport();
    let mut b = LayoutBuilder::new();
    if is_compact(vp) {
        let mut wins = compact_pair(&mut b, panels[0], panels[1], vp, margin);
        for kind in &panels[2..] {
            wins.push(docked_panel(&mut b, *kind));
        }
        return wins;
    }
    let rows = panels.len().div_ceil(columns);
    let inner_w = vp.width - 2.0 * margin - gap * (columns - 1) as f64;
    let inner_h = (vp.height - 2.0 * margin - gap * (rows - 1) as f64).max(360.0);
    let w = (inner_w / columns as f64).max(240.0);
    let h = (inner_h / rows as f64).max(220.0);
    let mut wins = Vec::with_capacity(panels.len());
    for (index, kind) in panels.iter().enumerate() {
        let col = (index % columns) as f64;
        let row = (index / columns) as f64;
        wins.push(b.at(
            *kind,
            margin + col * (w + gap),
            margin + row * (h + gap),
            w,
            h,
        ));
    }
    wins
}

/// Compact tier: the primary fills the workspace, the rest wait in the dock.
pub(crate) fn compact_defaults<K: PanelKind>(primary: K, docked: &[K]) -> Vec<PanelWin<K>> {
    let vp = current_viewport();
    let mut b = LayoutBuilder::new();
    let margin = 8.0;
    let mut wins = vec![b.at(
        primary,
        margin,
        margin,
        vp.width - 2.0 * margin,
        vp.height - 2.0 * margin,
    )];
    for kind in docked {
        wins.push(docked_panel(&mut b, *kind));
    }
    wins
}

/// Compact tier pair used by the shared default helpers.
fn compact_pair<K: PanelKind>(
    b: &mut LayoutBuilder,
    primary: K,
    secondary: K,
    vp: Viewport,
    margin: f64,
) -> Vec<PanelWin<K>> {
    let mut wins = vec![b.at(
        primary,
        margin,
        margin,
        vp.width - 2.0 * margin,
        vp.height - 2.0 * margin,
    )];
    wins.push(docked_panel(b, secondary));
    wins
}

/// A panel waiting in the dock: same rect as the primary (restore re-clamps).
fn docked_panel<K: PanelKind>(b: &mut LayoutBuilder, kind: K) -> PanelWin<K> {
    let mut win = b.at(kind, 0.0, 0.0, 0.0, 0.0);
    win.state = WinState::Minimized;
    win
}

fn restore_or_default<K: PanelKind>(
    storage_key: &str,
    store: &LocalStorageLayoutStore,
    defaults: Snapshot<K>,
    catalog: &PanelCatalog<K>,
) -> Snapshot<K> {
    let context = RestoreContext {
        units: Units::CssPx,
        viewport: (defaults.viewport.width, defaults.viewport.height),
    };

    match restore_snapshot(store, defaults.clone(), catalog, context) {
        Ok(snapshot) => snapshot,
        Err(error) => {
            log_layout_error("restore layout", storage_key, &error);
            defaults
        }
    }
}

fn log_layout_error(action: &str, storage_key: &str, error: &LayoutError) {
    let message = format!("panel-kit {action} failed for storage key `{storage_key}`: {error}");

    #[cfg(target_arch = "wasm32")]
    web_sys::console::error_1(&wasm_bindgen::JsValue::from_str(&message));

    #[cfg(not(target_arch = "wasm32"))]
    eprintln!("{message}");
}

#[cfg(test)]
mod tests {
    use super::*;
    use panel_kit::LayoutBuilder;
    use serde::{Deserialize, Serialize};

    #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
    enum TestPanel {
        First,
        Second,
        Third,
    }

    impl PanelKind for TestPanel {
        fn title(self) -> &'static str {
            match self {
                Self::First => "First",
                Self::Second => "Second",
                Self::Third => "Third",
            }
        }
    }

    #[test]
    fn compact_projection_stacks_in_snapshot_order() {
        let mut layout = LayoutBuilder::new();
        let defaults = vec![
            layout.at(TestPanel::First, 0.0, 0.0, 320.0, 240.0),
            layout.at(TestPanel::Second, 0.0, 0.0, 320.0, 240.0),
            layout.at(TestPanel::Third, 0.0, 0.0, 320.0, 240.0),
        ];
        let snapshot = Snapshot::from_defaults(
            defaults,
            Mode::Floating,
            Viewport {
                width: 375.0,
                height: 800.0,
                units: Units::CssPx,
            },
        );
        let mut scratch = ProjectionBuffer::with_panel_capacity(snapshot.panels.len());

        let frame = project_workspace(&snapshot, &mut scratch);
        let order = frame
            .panels
            .iter()
            .map(|panel| panel.key)
            .collect::<Vec<_>>();

        assert_eq!(frame.surface.class, SurfaceClass::Compact);
        assert_eq!(frame.mode, Mode::Tiling);
        assert_eq!(
            order,
            vec![TestPanel::First, TestPanel::Second, TestPanel::Third]
        );
    }
}
