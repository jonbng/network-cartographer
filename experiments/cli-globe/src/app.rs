//! Terminal application state and user actions.

use crate::{
    data::{color_for_key, Application, Density, Settings, Snapshot},
    gfx::{CellPx, GfxBackend},
    globe::{GlobeRenderer, Hop as GlobeHop, Path as GlobePath},
};
use ratatui::layout::Rect;
use std::collections::{BTreeSet, HashMap};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pane {
    Applications,
    Globe,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Overlay {
    None,
    Help,
    Debug,
    Settings,
    Privacy,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RowRef {
    App(String),
    Destination {
        app_id: String,
        destination_id: String,
    },
}

#[derive(Clone, Debug)]
pub enum Action {
    Quit,
    NextPane,
    PreviousPane,
    Up,
    Down,
    Left,
    Right,
    Activate,
    ToggleFocus,
    Clear,
    StartFilter,
    FilterChar(char),
    FilterBackspace,
    FilterClear,
    SubmitFilter,
    ToggleHelp,
    ToggleDebug,
    ToggleSettings,
    ToggleSetting(usize),
    AcceptPrivacy { local_only: bool },
    Recenter,
    TraceAll,
    Reset,
    CycleDensity,
    ToggleLabels,
    ToggleFlow,
    ZoomIn,
    ZoomOut,
    ZoomReset,
    ToggleSpin,
    SetBackend(GfxBackend),
}

#[derive(Clone, Debug)]
pub enum Effect {
    None,
    Quit,
    ApplySettings(Settings),
    TraceAll,
    Reset,
}

pub struct App {
    pub snapshot: Snapshot,
    pub globe: GlobeRenderer,
    pub backend: GfxBackend,
    pub cell_px: CellPx,
    pub pane: Pane,
    pub overlay: Overlay,
    pub selected: usize,
    pub expanded: BTreeSet<String>,
    pub focused: BTreeSet<String>,
    pub filter: String,
    pub filtering: bool,
    pub density: Density,
    pub status: String,
    pub spin: bool,
    pub animate_flow: bool,
    pub dragging: bool,
    pub drag_last: Option<(u16, u16)>,
    pub globe_inner: Rect,
    pub sidebar_inner: Rect,
    pub frame_px: (u32, u32),
    pub fps: f32,
    pub ui_dirty: bool,
    pub globe_dirty: bool,
    pub camera_moved: bool,
    pub last_frame: Option<image::RgbImage>,
    pub kitty_front: u32,
}

impl App {
    pub fn new(
        snapshot: Snapshot,
        mut globe: GlobeRenderer,
        backend: GfxBackend,
        cell_px: CellPx,
    ) -> Self {
        let density = snapshot.settings.density;
        let overlay = if !snapshot.demo && !snapshot.settings.privacy_accepted {
            Overlay::Privacy
        } else {
            Overlay::None
        };
        globe.set_zoom(default_zoom(backend));
        globe.compact_destination_markers = backend != GfxBackend::Kitty;
        let mut app = Self {
            snapshot,
            globe,
            backend,
            cell_px,
            pane: Pane::Applications,
            overlay,
            selected: 0,
            expanded: BTreeSet::new(),
            focused: BTreeSet::new(),
            filter: String::new(),
            filtering: false,
            density,
            status: "Ready".into(),
            spin: false,
            animate_flow: true,
            dragging: false,
            drag_last: None,
            globe_inner: Rect::default(),
            sidebar_inner: Rect::default(),
            frame_px: (0, 0),
            fps: 0.0,
            ui_dirty: true,
            globe_dirty: true,
            camera_moved: false,
            last_frame: None,
            kitty_front: 42,
        };
        app.rebuild_globe_paths(true);
        app
    }

    pub fn apply_snapshot(&mut self, snapshot: Snapshot) {
        let first_real_data =
            self.snapshot.mapped_path_count() == 0 && snapshot.mapped_path_count() > 0;
        self.density = snapshot.settings.density;
        self.snapshot = snapshot;
        if self.overlay == Overlay::Privacy && self.snapshot.settings.privacy_accepted {
            self.overlay = Overlay::None;
        } else if !self.snapshot.demo && !self.snapshot.settings.privacy_accepted {
            self.overlay = Overlay::Privacy;
        }
        self.clamp_selection();
        self.rebuild_globe_paths(first_real_data && !self.camera_moved);
        self.ui_dirty = true;
    }

    pub fn visible_rows(&self) -> Vec<RowRef> {
        let mut rows = Vec::new();
        for app in self.filtered_apps() {
            rows.push(RowRef::App(app.id.clone()));
            if self.expanded.contains(&app.id) || self.focused.contains(&app.id) {
                for destination in &app.destinations {
                    if self.destination_matches(app, destination) {
                        rows.push(RowRef::Destination {
                            app_id: app.id.clone(),
                            destination_id: destination.id.clone(),
                        });
                    }
                }
            }
        }
        rows
    }

    pub fn filtered_apps(&self) -> Vec<&Application> {
        let query = self.query().to_lowercase();
        self.snapshot
            .apps
            .iter()
            .filter(|app| {
                query.is_empty()
                    || app.name.to_lowercase().contains(&query)
                    || app
                        .destinations
                        .iter()
                        .any(|destination| self.destination_matches(app, destination))
            })
            .collect()
    }

    pub fn selected_row(&self) -> Option<RowRef> {
        self.visible_rows().get(self.selected).cloned()
    }

    pub fn selected_app(&self) -> Option<&Application> {
        let id = match self.selected_row()? {
            RowRef::App(id) => id,
            RowRef::Destination { app_id, .. } => app_id,
        };
        self.snapshot.apps.iter().find(|app| app.id == id)
    }

    pub fn selected_destination(&self) -> Option<(&Application, &crate::data::Destination)> {
        let RowRef::Destination {
            app_id,
            destination_id,
        } = self.selected_row()?
        else {
            return None;
        };
        let app = self.snapshot.apps.iter().find(|app| app.id == app_id)?;
        let destination = app
            .destinations
            .iter()
            .find(|destination| destination.id == destination_id)?;
        Some((app, destination))
    }

    pub fn update(&mut self, action: Action) -> Effect {
        if self.overlay == Overlay::Privacy {
            return match action {
                Action::AcceptPrivacy { local_only } => {
                    self.snapshot.settings.privacy_accepted = true;
                    self.snapshot.settings.geo_local_only = local_only;
                    self.overlay = Overlay::None;
                    self.status = if local_only {
                        "Privacy accepted · local GeoIP only".into()
                    } else {
                        "Privacy accepted · live monitoring starting".into()
                    };
                    self.ui_dirty = true;
                    Effect::ApplySettings(self.snapshot.settings.clone())
                }
                Action::Quit => Effect::Quit,
                _ => Effect::None,
            };
        }

        match action {
            Action::Quit => return Effect::Quit,
            Action::ToggleHelp => self.toggle_overlay(Overlay::Help),
            Action::ToggleDebug => self.toggle_overlay(Overlay::Debug),
            Action::ToggleSettings => self.toggle_overlay(Overlay::Settings),
            Action::Clear if self.overlay != Overlay::None => {
                self.overlay = Overlay::None;
                self.ui_dirty = true;
            }
            Action::ToggleSetting(index) if self.overlay == Overlay::Settings => {
                match index {
                    1 => {
                        self.snapshot.settings.external_only = !self.snapshot.settings.external_only
                    }
                    2 => {
                        self.snapshot.settings.traces_enabled =
                            !self.snapshot.settings.traces_enabled
                    }
                    3 => {
                        self.snapshot.settings.geo_local_only =
                            !self.snapshot.settings.geo_local_only
                    }
                    4 => {
                        self.snapshot.settings.history_enabled =
                            !self.snapshot.settings.history_enabled
                    }
                    _ => return Effect::None,
                }
                self.status = "Settings updated".into();
                self.ui_dirty = true;
                return Effect::ApplySettings(self.snapshot.settings.clone());
            }
            _ if self.overlay != Overlay::None => return Effect::None,
            Action::NextPane | Action::PreviousPane => {
                self.pane = match self.pane {
                    Pane::Applications => Pane::Globe,
                    Pane::Globe => Pane::Applications,
                };
                self.ui_dirty = true;
            }
            Action::Up if self.pane == Pane::Applications => self.move_selection(-1),
            Action::Down if self.pane == Pane::Applications => self.move_selection(1),
            Action::Left if self.pane == Pane::Globe => self.orbit(-0.12, 0.0),
            Action::Right if self.pane == Pane::Globe => self.orbit(0.12, 0.0),
            Action::Up if self.pane == Pane::Globe => self.orbit(0.0, 0.08),
            Action::Down if self.pane == Pane::Globe => self.orbit(0.0, -0.08),
            Action::Activate if self.pane == Pane::Applications => self.activate_selected(),
            Action::ToggleFocus if self.pane == Pane::Applications => self.toggle_selected_focus(),
            Action::Clear => {
                if !self.focused.is_empty() {
                    self.focused.clear();
                    self.status = "Showing all applications".into();
                    self.rebuild_globe_paths(false);
                } else if !self.filter.is_empty() {
                    self.filter.clear();
                    self.rebuild_globe_paths(false);
                }
                self.ui_dirty = true;
            }
            Action::StartFilter => {
                self.filtering = true;
                self.pane = Pane::Applications;
                self.ui_dirty = true;
            }
            Action::FilterChar(ch) => {
                self.filter.push(ch);
                self.selected = 0;
                self.rebuild_globe_paths(false);
            }
            Action::FilterBackspace => {
                self.filter.pop();
                self.selected = 0;
                self.rebuild_globe_paths(false);
            }
            Action::FilterClear => {
                self.filter.clear();
                self.selected = 0;
                self.rebuild_globe_paths(false);
            }
            Action::SubmitFilter => {
                self.filtering = false;
                self.ui_dirty = true;
            }
            Action::Recenter => {
                self.globe.frame_paths(default_zoom(self.backend));
                self.camera_moved = false;
                self.status = "Camera recentered on active paths".into();
                self.globe_dirty = true;
            }
            Action::TraceAll => {
                self.status = "Re-tracing all destinations…".into();
                self.ui_dirty = true;
                return Effect::TraceAll;
            }
            Action::Reset => {
                self.focused.clear();
                self.expanded.clear();
                self.status = "Monitor and traceroute cache reset".into();
                self.ui_dirty = true;
                return Effect::Reset;
            }
            Action::CycleDensity => {
                self.density = self.density.next();
                self.snapshot.settings.density = self.density;
                self.status = format!("Globe density · {}", self.density.label());
                self.rebuild_globe_paths(false);
                return Effect::ApplySettings(self.snapshot.settings.clone());
            }
            Action::ToggleLabels => {
                self.globe.show_labels = !self.globe.show_labels;
                self.status = if self.globe.show_labels {
                    "Destination labels on".into()
                } else {
                    "Destination labels off".into()
                };
                self.globe_dirty = true;
            }
            Action::ToggleFlow => {
                self.animate_flow = !self.animate_flow;
                self.status = if self.animate_flow {
                    "Data-flow animation on".into()
                } else {
                    "Data-flow animation off".into()
                };
                self.globe_dirty = true;
                self.ui_dirty = true;
            }
            Action::ZoomIn => self.zoom(1.12),
            Action::ZoomOut => self.zoom(1.0 / 1.12),
            Action::ZoomReset => {
                self.globe.set_zoom(default_zoom(self.backend));
                self.camera_moved = true;
                self.globe_dirty = true;
            }
            Action::ToggleSpin => {
                self.spin = !self.spin;
                self.status = if self.spin {
                    "Auto-rotate on"
                } else {
                    "Auto-rotate off"
                }
                .into();
                self.ui_dirty = true;
            }
            Action::SetBackend(backend) => self.set_backend(backend),
            Action::AcceptPrivacy { .. } | Action::ToggleSetting(_) => {}
            Action::Left | Action::Right | Action::Activate | Action::ToggleFocus => {}
            Action::Up | Action::Down => {}
        }
        Effect::None
    }

    pub fn orbit_drag(&mut self, dx: i32, dy: i32) {
        let sensitivity = 0.045 / self.globe.zoom.sqrt();
        self.globe.yaw -= dx as f32 * sensitivity;
        self.globe.pitch = (self.globe.pitch + dy as f32 * sensitivity * 0.8).clamp(-1.2, 1.2);
        self.camera_moved = true;
        self.spin = false;
        self.globe_dirty = true;
    }

    fn query(&self) -> &str {
        self.filter.trim()
    }

    fn destination_matches(
        &self,
        app: &Application,
        destination: &crate::data::Destination,
    ) -> bool {
        let query = self.query().to_lowercase();
        if query.is_empty() || app.name.to_lowercase().contains(&query) {
            return true;
        }
        destination.host.to_lowercase().contains(&query)
            || destination.ip.contains(&query)
            || destination
                .org
                .as_deref()
                .unwrap_or_default()
                .to_lowercase()
                .contains(&query)
            || destination.hops.iter().any(|hop| {
                hop.city
                    .as_deref()
                    .unwrap_or_default()
                    .to_lowercase()
                    .contains(&query)
                    || hop
                        .addr
                        .as_deref()
                        .unwrap_or_default()
                        .to_lowercase()
                        .contains(&query)
            })
    }

    fn rebuild_globe_paths(&mut self, frame: bool) {
        let query = self.query().to_lowercase();
        let mut hub_counts: HashMap<(i32, i32), usize> = HashMap::new();
        for destination in self.snapshot.apps.iter().flat_map(|app| &app.destinations) {
            for hop in &destination.hops {
                if let (Some(lat), Some(lon)) = (hop.lat, hop.lon) {
                    *hub_counts
                        .entry(((lat * 100.0) as i32, (lon * 100.0) as i32))
                        .or_default() += 1;
                }
            }
        }

        let mut paths = Vec::new();
        for application in &self.snapshot.apps {
            for destination in &application.destinations {
                let matches = query.is_empty()
                    || application.name.to_lowercase().contains(&query)
                    || destination.host.to_lowercase().contains(&query)
                    || destination.ip.contains(&query)
                    || destination
                        .org
                        .as_deref()
                        .unwrap_or_default()
                        .to_lowercase()
                        .contains(&query)
                    || destination.hops.iter().any(|hop| {
                        hop.city
                            .as_deref()
                            .unwrap_or_default()
                            .to_lowercase()
                            .contains(&query)
                            || hop
                                .addr
                                .as_deref()
                                .unwrap_or_default()
                                .to_lowercase()
                                .contains(&query)
                    });
                if !matches || destination.status != "done" {
                    continue;
                }
                let located: Vec<_> = destination
                    .hops
                    .iter()
                    .filter_map(|hop| Some((hop, hop.lat?, hop.lon?)))
                    .collect();
                if located.is_empty() {
                    continue;
                }
                let start = if self.density == Density::Destinations && located.len() > 2 {
                    located.len() - 2
                } else {
                    0
                };
                let hops = located[start..]
                    .iter()
                    .enumerate()
                    .map(|(index, (hop, lat, lon))| {
                        let is_destination = index + 1 == located[start..].len();
                        let is_hub = hub_counts
                            .get(&((lat * 100.0) as i32, (lon * 100.0) as i32))
                            .copied()
                            .unwrap_or(0)
                            > 1;
                        GlobeHop {
                            lat: *lat,
                            lon: *lon,
                            label: if is_destination {
                                match (&hop.city, &hop.country) {
                                    (Some(city), Some(country)) => format!("{city}, {country}"),
                                    (Some(city), None) => city.clone(),
                                    _ => destination.host.clone(),
                                }
                            } else {
                                hop.city.clone().unwrap_or_default()
                            },
                            show_marker: match self.density {
                                Density::All => true,
                                Density::Destinations => is_destination,
                                Density::Hubs => is_destination || is_hub,
                            },
                        }
                    })
                    .collect();
                paths.push(GlobePath {
                    app_id: application.id.clone(),
                    color: color_for_key(&application.name),
                    hops,
                });
            }
        }
        self.globe.paths = paths;
        self.globe.focused_apps = self.focused.clone();
        if frame && !self.globe.paths.is_empty() {
            self.globe.frame_paths(default_zoom(self.backend));
        }
        self.globe_dirty = true;
        self.ui_dirty = true;
    }

    fn move_selection(&mut self, delta: isize) {
        let count = self.visible_rows().len();
        if count == 0 {
            self.selected = 0;
            return;
        }
        self.selected = (self.selected as isize + delta).clamp(0, count as isize - 1) as usize;
        self.ui_dirty = true;
    }

    fn clamp_selection(&mut self) {
        self.selected = self
            .selected
            .min(self.visible_rows().len().saturating_sub(1));
    }

    fn activate_selected(&mut self) {
        let Some(row) = self.selected_row() else {
            return;
        };
        let app_id = match row {
            RowRef::App(id) => {
                if self.expanded.contains(&id)
                    && self.focused.len() == 1
                    && self.focused.contains(&id)
                {
                    self.expanded.remove(&id);
                    self.focused.clear();
                } else {
                    self.expanded.insert(id.clone());
                    self.focused.clear();
                    self.focused.insert(id.clone());
                }
                id
            }
            RowRef::Destination { app_id, .. } => {
                self.focused.clear();
                self.focused.insert(app_id.clone());
                app_id
            }
        };
        let name = self
            .snapshot
            .apps
            .iter()
            .find(|app| app.id == app_id)
            .map(|app| app.name.as_str())
            .unwrap_or("application");
        self.status = format!("Focused · {name}");
        self.rebuild_globe_paths(false);
    }

    fn toggle_selected_focus(&mut self) {
        let Some(row) = self.selected_row() else {
            return;
        };
        let app_id = match row {
            RowRef::App(id) => id,
            RowRef::Destination { app_id, .. } => app_id,
        };
        if !self.focused.remove(&app_id) {
            self.focused.insert(app_id.clone());
            self.expanded.insert(app_id);
        }
        self.status = if self.focused.is_empty() {
            "Showing all applications".into()
        } else {
            format!("Focused · {} applications", self.focused.len())
        };
        self.rebuild_globe_paths(false);
    }

    fn orbit(&mut self, yaw: f32, pitch: f32) {
        self.globe.yaw += yaw;
        self.globe.pitch = (self.globe.pitch + pitch).clamp(-1.2, 1.2);
        self.camera_moved = true;
        self.spin = false;
        self.globe_dirty = true;
    }

    fn zoom(&mut self, factor: f32) {
        self.globe.zoom_by(factor);
        self.camera_moved = true;
        self.status = format!("Zoom · {:.1}×", self.globe.zoom);
        self.globe_dirty = true;
        self.ui_dirty = true;
    }

    fn set_backend(&mut self, backend: GfxBackend) {
        self.backend = backend;
        self.globe.set_zoom(default_zoom(backend));
        self.globe.compact_destination_markers = backend != GfxBackend::Kitty;
        self.frame_px = (0, 0);
        self.last_frame = None;
        self.globe_dirty = true;
        self.ui_dirty = true;
        self.status = format!("Graphics backend · {}", backend.label());
    }

    fn toggle_overlay(&mut self, overlay: Overlay) {
        self.overlay = if self.overlay == overlay {
            Overlay::None
        } else {
            overlay
        };
        self.ui_dirty = true;
    }
}

pub fn default_zoom(backend: GfxBackend) -> f32 {
    GlobeRenderer::default_zoom_for_backend(
        backend == GfxBackend::Kitty,
        backend == GfxBackend::Braille,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::DynamicImage;

    fn demo_app() -> App {
        App::new(
            crate::mock::demo_snapshot(),
            GlobeRenderer::from_image(DynamicImage::new_rgb8(4, 4)),
            GfxBackend::Braille,
            CellPx::default(),
        )
    }

    #[test]
    fn activating_an_app_expands_and_focuses_its_routes() {
        let mut app = demo_app();
        let _ = app.update(Action::Activate);
        assert!(app.focused.contains("firefox"));
        assert!(app.expanded.contains("firefox"));
        assert_eq!(app.visible_rows().len(), 6);
        assert_eq!(app.globe.focused_apps, app.focused);
    }

    #[test]
    fn city_filter_drives_sidebar_and_globe_from_the_same_query() {
        let mut app = demo_app();
        let _ = app.update(Action::StartFilter);
        for ch in "dublin".chars() {
            let _ = app.update(Action::FilterChar(ch));
        }
        assert_eq!(app.filtered_apps().len(), 1);
        assert_eq!(app.filtered_apps()[0].name, "Spotify");
        assert_eq!(app.globe.paths.len(), 1);
        assert_eq!(app.globe.paths[0].app_id, "spotify");
    }

    #[test]
    fn flow_animation_can_be_disabled() {
        let mut app = demo_app();
        assert!(app.globe.compact_destination_markers);
        assert!(app.animate_flow);
        let _ = app.update(Action::ToggleFlow);
        assert!(!app.animate_flow);
        assert_eq!(app.status, "Data-flow animation off");

        let _ = app.update(Action::SetBackend(GfxBackend::Kitty));
        assert!(!app.globe.compact_destination_markers);
    }
}
