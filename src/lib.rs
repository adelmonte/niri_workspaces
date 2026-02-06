use gtk::prelude::*;
use niri_ipc::socket::Socket;
use niri_ipc::{Action, Event, Request, Response, WorkspaceReferenceArg};
use std::collections::HashMap;
use std::thread;
use waybar_cffi::serde::Deserialize;
use waybar_cffi::{gtk, waybar_module, InitInfo, Module};

#[derive(Debug, Clone, Deserialize)]
#[serde(crate = "waybar_cffi::serde")]
struct IgnoreRule {
    app_id: Option<String>,
    title: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(crate = "waybar_cffi::serde")]
struct FormatIcons {
    #[serde(default)]
    urgent: Option<String>,
    #[serde(default)]
    empty: Option<String>,
    #[serde(default)]
    focused: Option<String>,
    #[serde(default)]
    active: Option<String>,
    #[serde(default)]
    default: Option<String>,
    #[serde(flatten)]
    named: HashMap<String, String>,
}

struct NiriWorkspaces {
    container: gtk::Box,
    config: Config,
}

impl NiriWorkspaces {
    fn populate_workspaces(&self) {
        // Get workspace and window information
        let (workspaces, window_counts) = match get_workspace_info(&self.config.ignore_rules) {
            Ok(data) => data,
            Err(e) => {
                eprintln!("Failed to get workspace info: {}", e);
                return;
            }
        };

        // Get output name from the first workspace (if any)
        let output_name = if self.config.all_outputs {
            None // Don't filter by output
        } else {
            workspaces
                .first()
                .and_then(|ws| ws.output.clone())
        };

        // Find the highest workspace index with windows on this output (or all outputs)
        let max_workspace_idx = workspaces
            .iter()
            .filter(|ws| {
                if self.config.all_outputs {
                    window_counts.get(&ws.id).copied().unwrap_or(0) > 0
                } else {
                    ws.output == output_name && window_counts.get(&ws.id).copied().unwrap_or(0) > 0
                }
            })
            .map(|ws| ws.idx)
            .max()
            .unwrap_or(0);

        // Filter and sort workspaces
        let mut our_workspaces: Vec<_> = workspaces
            .into_iter()
            .filter(|ws| {
                // Filter by output unless all_outputs is enabled
                if !self.config.all_outputs && ws.output != output_name {
                    return false;
                }

                let has_windows = window_counts.get(&ws.id).copied().unwrap_or(0) > 0;

                // Show workspaces with windows, or the next empty workspace based on config
                if has_windows {
                    true
                } else if self.config.show_empty_workspace {
                    ws.idx == max_workspace_idx + 1  // Always show next empty workspace
                } else {
                    ws.idx == max_workspace_idx + 1 && ws.is_active  // Only show when active
                }
            })
            .collect();

        // Sort by workspace index to maintain consistent order
        our_workspaces.sort_by_key(|ws| ws.idx);

        // Get existing buttons
        let existing_buttons = self.container.children();

        // Check if we need to rebuild: count changed OR workspace IDs changed
        let need_rebuild = existing_buttons.len() != our_workspaces.len() || {
            existing_buttons.iter()
                .zip(our_workspaces.iter())
                .any(|(button, ws)| {
                    button.downcast_ref::<gtk::Button>()
                        .and_then(|b| unsafe { b.data::<u64>("ws_id").map(|ptr| *ptr.as_ptr()) })
                        .map_or(true, |stored_id| stored_id != ws.id)
                })
        };

        if need_rebuild {
            // Clear existing buttons
            for child in existing_buttons {
                self.container.remove(&child);
            }

            // Create new buttons for each workspace
            for ws in &our_workspaces {
                let button = gtk::Button::new();
                // Add CSS class for styling
                button.style_context().add_class("workspace-button");
                // Enable markup for colored icons and center align
                if let Some(label) = button.child().and_then(|w| w.downcast::<gtk::Label>().ok()) {
                    label.set_use_markup(true);
                    label.set_xalign(0.5);
                    label.set_yalign(0.5);
                    label.set_halign(gtk::Align::Center);
                    label.set_valign(gtk::Align::Center);
                }

                // Store workspace index for drag-and-drop
                unsafe { button.set_data("ws_idx", ws.idx); }

                // Set up drag-and-drop for workspace reordering
                setup_workspace_drag_drop(&button, ws.id);

                self.container.add(&button);
            }
        }

        // Update all buttons with current state
        let buttons = self.container.children();
        for (button, ws) in buttons.iter().zip(our_workspaces.iter()) {
            if let Some(button) = button.downcast_ref::<gtk::Button>() {
                let window_count = window_counts.get(&ws.id).copied().unwrap_or(0);

                // Determine the display value
                let value = if let Some(name) = &ws.name {
                    name.clone()
                } else {
                    ws.idx.to_string()
                };

                // Get the icon (either from format-icons or pie chart)
                let icon = if self.config.format_icons.is_some() {
                    get_format_icon(ws, self.config.format_icons.as_ref(), &value)
                } else {
                    get_pie_icon(window_count, self.config.icon_size.as_deref())
                };

                // Build the label using format string or default to icon
                // Escape user-controlled data to prevent markup injection
                let escaped_value = gtk::glib::markup_escape_text(&value);
                let escaped_name = gtk::glib::markup_escape_text(ws.name.as_deref().unwrap_or(""));
                let escaped_index = gtk::glib::markup_escape_text(&ws.idx.to_string());
                let escaped_output = gtk::glib::markup_escape_text(ws.output.as_deref().unwrap_or(""));

                let label_text = if let Some(format) = &self.config.format {
                    format
                        .replace("{icon}", &icon)  // Icon is safe (hardcoded markup or user-provided)
                        .replace("{value}", &escaped_value)
                        .replace("{name}", &escaped_name)
                        .replace("{index}", &escaped_index)
                        .replace("{output}", &escaped_output)
                } else {
                    icon
                };

                // Update button label - always use markup for pie chart icons
                button.set_label(&label_text);
                if let Some(label) = button.child().and_then(|w| w.downcast::<gtk::Label>().ok()) {
                    label.set_use_markup(true);
                }

                // Set button name for CSS targeting
                button.set_widget_name(&format!("niri-workspace-{}", value));

                let style_context = button.style_context();

                // Update CSS classes based on workspace state
                Self::update_css_class(&style_context, "focused", ws.is_focused);
                Self::update_css_class(&style_context, "active", ws.is_active);
                Self::update_css_class(&style_context, "urgent", ws.is_urgent);
                Self::update_css_class(&style_context, "empty", ws.active_window_id.is_none());

                // Add current_output class if workspace is on the same output as the bar
                if let Some(ref bar_output) = output_name {
                    Self::update_css_class(&style_context, "current_output",
                        ws.output.as_ref() == Some(bar_output));
                }

                // Handle current_only visibility
                if self.config.current_only {
                    if self.config.all_outputs {
                        button.set_visible(ws.is_focused);
                    } else {
                        button.set_visible(ws.is_active);
                    }
                } else {
                    button.set_visible(true);
                }

                // Store workspace ID for click handler (only if not already set)
                unsafe {
                    if button.data::<u64>("ws_id").is_none() {
                        if !self.config.disable_click {
                            let ws_id = ws.id;
                            button.connect_clicked(move |_| {
                                focus_workspace(ws_id);
                            });
                        }
                        button.set_data("ws_id", ws.id);
                    }
                }
            }
        }

        self.container.show_all();
    }

    fn update_css_class(style_context: &gtk::StyleContext, class: &str, should_have: bool) {
        if should_have {
            if !style_context.has_class(class) {
                style_context.add_class(class);
            }
        } else {
            if style_context.has_class(class) {
                style_context.remove_class(class);
            }
        }
    }
}

impl Module for NiriWorkspaces {
    type Config = Config;

    fn init(info: &InitInfo, config: Config) -> Self {
        // Load CSS for drag-and-drop styles
        let css_provider = gtk::CssProvider::new();
        let css = b"
            .workspace-button.dragging {
                opacity: 0.6;
                background-color: rgba(102, 204, 255, 0.3);
            }
            .workspace-button.drag-over {
                background-color: rgba(102, 255, 153, 0.2);
                border: 1px dashed rgba(102, 255, 153, 0.6);
            }
        ";
        if let Err(e) = css_provider.load_from_data(css) {
            eprintln!("Failed to load CSS: {}", e);
        }
        gtk::StyleContext::add_provider_for_screen(
            &gtk::gdk::Screen::default().expect("Failed to get default screen"),
            &css_provider,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );

        let container = gtk::Box::new(gtk::Orientation::Horizontal, 0);

        let root = info.get_root_widget();
        root.add(&container);

        let module = Self {
            container,
            config: config.clone(),
        };

        // Populate initial workspace buttons
        module.populate_workspaces();

        // Set up event stream listener using glib channel
        #[allow(deprecated)]
        let (tx, rx) = gtk::glib::MainContext::channel(gtk::glib::Priority::DEFAULT);

        thread::spawn(move || {
            loop {
                // Connect to event stream
                let mut socket = match Socket::connect() {
                    Ok(s) => s,
                    Err(e) => {
                        eprintln!("Failed to connect to niri socket: {}", e);
                        thread::sleep(std::time::Duration::from_secs(5));
                        continue;
                    }
                };

                // Request event stream
                let reply = match socket.send(Request::EventStream) {
                    Ok(r) => r,
                    Err(e) => {
                        eprintln!("Failed to request event stream: {}", e);
                        thread::sleep(std::time::Duration::from_secs(5));
                        continue;
                    }
                };

                if let Err(e) = reply {
                    eprintln!("Event stream request failed: {}", e);
                    thread::sleep(std::time::Duration::from_secs(5));
                    continue;
                }

                // Start reading events
                let mut read_event = socket.read_events();

                // Listen for events
                loop {
                    match read_event() {
                        Ok(event) => {
                            // Only signal update on workspace or window changes
                            match event {
                                Event::WorkspacesChanged { .. }
                                | Event::WorkspaceActivated { .. }
                                | Event::WindowOpenedOrChanged { .. }
                                | Event::WindowClosed { .. } => {
                                    let _ = tx.send(());
                                }
                                _ => {}
                            }
                        }
                        Err(e) => {
                            eprintln!("Error reading event: {}", e);
                            break;
                        }
                    }
                }

                // Reconnect after a delay if connection is lost
                thread::sleep(std::time::Duration::from_secs(1));
            }
        });

        // Listen for update signals from the event thread
        let container_clone = module.container.clone();
        let config_clone = config.clone();
        rx.attach(None, move |_| {
            // Get workspace and window information
            let (workspaces, window_counts) = match get_workspace_info(&config_clone.ignore_rules) {
                Ok(data) => data,
                Err(_) => return gtk::glib::ControlFlow::Continue,
            };

            // Get output name from the first workspace (if any)
            let output_name = if config_clone.all_outputs {
                None // Don't filter by output
            } else {
                workspaces
                    .first()
                    .and_then(|ws| ws.output.clone())
            };

            // Find the highest workspace index with windows on this output (or all outputs)
            let max_workspace_idx = workspaces
                .iter()
                .filter(|ws| {
                    if config_clone.all_outputs {
                        window_counts.get(&ws.id).copied().unwrap_or(0) > 0
                    } else {
                        ws.output == output_name && window_counts.get(&ws.id).copied().unwrap_or(0) > 0
                    }
                })
                .map(|ws| ws.idx)
                .max()
                .unwrap_or(0);

            // Filter and sort workspaces
            let mut our_workspaces: Vec<_> = workspaces
                .into_iter()
                .filter(|ws| {
                    // Filter by output unless all_outputs is enabled
                    if !config_clone.all_outputs && ws.output != output_name {
                        return false;
                    }

                    let has_windows = window_counts.get(&ws.id).copied().unwrap_or(0) > 0;

                    // Show workspaces with windows, or the next empty workspace based on config
                    if has_windows {
                        true
                    } else if config_clone.show_empty_workspace {
                        ws.idx == max_workspace_idx + 1  // Always show next empty workspace
                    } else {
                        ws.idx == max_workspace_idx + 1 && ws.is_active  // Only show when active
                    }
                })
                .collect();

            // Sort by workspace index to maintain consistent order
            our_workspaces.sort_by_key(|ws| ws.idx);

            // Get existing buttons
            let existing_buttons = container_clone.children();

            // Check if we need to rebuild: count changed OR workspace IDs changed
            let need_rebuild = existing_buttons.len() != our_workspaces.len() || {
                existing_buttons.iter()
                    .zip(our_workspaces.iter())
                    .any(|(button, ws)| {
                        button.downcast_ref::<gtk::Button>()
                            .and_then(|b| unsafe { b.data::<u64>("ws_id").map(|ptr| *ptr.as_ptr()) })
                            .map_or(true, |stored_id| stored_id != ws.id)
                    })
            };

            if need_rebuild {
                // Clear existing buttons
                for child in existing_buttons {
                    container_clone.remove(&child);
                }

                // Create new buttons for each workspace with initial state
                for ws in &our_workspaces {
                    let button = gtk::Button::new();
                    let ws_id = ws.id;

                    // Add CSS class for styling
                    button.style_context().add_class("workspace-button");

                    // Set initial label
                    let window_count = window_counts.get(&ws.id).copied().unwrap_or(0);
                    let value = if let Some(name) = &ws.name {
                        name.clone()
                    } else {
                        ws.idx.to_string()
                    };

                    let icon = if config_clone.format_icons.is_some() {
                        get_format_icon(ws, config_clone.format_icons.as_ref(), &value)
                    } else {
                        get_pie_icon(window_count, config_clone.icon_size.as_deref())
                    };

                    // Escape user-controlled data to prevent markup injection
                    let escaped_value = gtk::glib::markup_escape_text(&value);
                    let escaped_name = gtk::glib::markup_escape_text(ws.name.as_deref().unwrap_or(""));
                    let escaped_index = gtk::glib::markup_escape_text(&ws.idx.to_string());
                    let escaped_output = gtk::glib::markup_escape_text(ws.output.as_deref().unwrap_or(""));

                    let label_text = if let Some(format) = &config_clone.format {
                        format
                            .replace("{icon}", &icon)  // Icon is safe (hardcoded markup or user-provided)
                            .replace("{value}", &escaped_value)
                            .replace("{name}", &escaped_name)
                            .replace("{index}", &escaped_index)
                            .replace("{output}", &escaped_output)
                    } else {
                        icon
                    };

                    button.set_label(&label_text);

                    // Enable markup for colored icons and center align
                    if let Some(label) = button.child().and_then(|w| w.downcast::<gtk::Label>().ok()) {
                        label.set_use_markup(true);
                        label.set_xalign(0.5);
                        label.set_yalign(0.5);
                        label.set_halign(gtk::Align::Center);
                        label.set_valign(gtk::Align::Center);
                    }

                    // Store workspace ID and index for drag-drop
                    unsafe {
                        button.set_data("ws_id", ws_id);
                        button.set_data("ws_idx", ws.idx);
                    }

                    // Set button name for CSS targeting
                    button.set_widget_name(&format!("niri-workspace-{}", value));

                    // Set initial CSS classes
                    let style_context = button.style_context();
                    if ws.is_focused {
                        style_context.add_class("focused");
                    }
                    if ws.is_active {
                        style_context.add_class("active");
                    }
                    if ws.is_urgent {
                        style_context.add_class("urgent");
                    }
                    if ws.active_window_id.is_none() {
                        style_context.add_class("empty");
                    }
                    if let Some(ref bar_output) = output_name {
                        if ws.output.as_ref() == Some(bar_output) {
                            style_context.add_class("current_output");
                        }
                    }

                    // Set up drag-and-drop for workspace reordering
                    setup_workspace_drag_drop(&button, ws.id);

                    // Set up click handler if not disabled
                    if !config_clone.disable_click {
                        button.connect_clicked(move |_| {
                            focus_workspace(ws_id);
                        });
                    }

                    container_clone.add(&button);
                }
            }

            // Update all buttons with current state
            let buttons = container_clone.children();
            for (button, ws) in buttons.iter().zip(our_workspaces.iter()) {
                if let Some(button) = button.downcast_ref::<gtk::Button>() {
                    let window_count = window_counts.get(&ws.id).copied().unwrap_or(0);

                    // Determine the display value
                    let value = if let Some(name) = &ws.name {
                        name.clone()
                    } else {
                        ws.idx.to_string()
                    };

                    // Get the icon (either from format-icons or pie chart)
                    let icon = if config_clone.format_icons.is_some() {
                        get_format_icon(ws, config_clone.format_icons.as_ref(), &value)
                    } else {
                        get_pie_icon(window_count, config_clone.icon_size.as_deref())
                    };

                    // Build the label using format string or default to icon
                    // Escape user-controlled data to prevent markup injection
                    let escaped_value = gtk::glib::markup_escape_text(&value);
                    let escaped_name = gtk::glib::markup_escape_text(ws.name.as_deref().unwrap_or(""));
                    let escaped_index = gtk::glib::markup_escape_text(&ws.idx.to_string());
                    let escaped_output = gtk::glib::markup_escape_text(ws.output.as_deref().unwrap_or(""));

                    let label_text = if let Some(format) = &config_clone.format {
                        format
                            .replace("{icon}", &icon)  // Icon is safe (hardcoded markup or user-provided)
                            .replace("{value}", &escaped_value)
                            .replace("{name}", &escaped_name)
                            .replace("{index}", &escaped_index)
                            .replace("{output}", &escaped_output)
                    } else {
                        icon
                    };

                    // Update button label - always use markup for pie chart icons
                    button.set_label(&label_text);
                    if let Some(label) = button.child().and_then(|w| w.downcast::<gtk::Label>().ok()) {
                        label.set_use_markup(true);
                    }

                    // Set button name for CSS targeting
                    button.set_widget_name(&format!("niri-workspace-{}", value));

                    let style_context = button.style_context();

                    // Update CSS classes using helper function (defined in impl block)
                    // Since we're in a closure, we need to manually update classes
                    let update_class = |class: &str, should_have: bool| {
                        if should_have {
                            if !style_context.has_class(class) {
                                style_context.add_class(class);
                            }
                        } else {
                            if style_context.has_class(class) {
                                style_context.remove_class(class);
                            }
                        }
                    };

                    update_class("focused", ws.is_focused);
                    update_class("active", ws.is_active);
                    update_class("urgent", ws.is_urgent);
                    update_class("empty", ws.active_window_id.is_none());

                    // Add current_output class if workspace is on the same output as the bar
                    if let Some(ref bar_output) = output_name {
                        update_class("current_output", ws.output.as_ref() == Some(bar_output));
                    }

                    // Handle current_only visibility
                    if config_clone.current_only {
                        if config_clone.all_outputs {
                            button.set_visible(ws.is_focused);
                        } else {
                            button.set_visible(ws.is_active);
                        }
                    } else {
                        button.set_visible(true);
                    }
                }
            }

            container_clone.show_all();
            gtk::glib::ControlFlow::Continue
        });

        root.show_all();

        module
    }

    fn update(&mut self) {
        self.populate_workspaces();
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(crate = "waybar_cffi::serde")]
struct Config {
    #[serde(default)]
    ignore_rules: Vec<IgnoreRule>,
    #[serde(default = "default_show_empty_workspace")]
    show_empty_workspace: bool,
    #[serde(default)]
    icon_size: Option<String>,
    #[serde(default)]
    all_outputs: bool,
    #[serde(default)]
    format: Option<String>,
    #[serde(default, rename = "format-icons")]
    format_icons: Option<FormatIcons>,
    #[serde(default)]
    disable_click: bool,
    #[serde(default)]
    current_only: bool,
}

fn default_show_empty_workspace() -> bool {
    true
}

fn get_workspace_info(ignore_rules: &[IgnoreRule]) -> Result<(Vec<niri_ipc::Workspace>, HashMap<u64, usize>), String> {
    // Get workspaces
    let mut socket = Socket::connect().map_err(|e| e.to_string())?;
    let reply = socket.send(Request::Workspaces).map_err(|e| e.to_string())?;
    let workspaces = match reply {
        Ok(Response::Workspaces(ws)) => ws,
        Ok(_) => return Err("Unexpected response type".to_string()),
        Err(e) => return Err(e),
    };

    // Get windows
    let mut socket = Socket::connect().map_err(|e| e.to_string())?;
    let reply = socket.send(Request::Windows).map_err(|e| e.to_string())?;
    let windows = match reply {
        Ok(Response::Windows(w)) => w,
        Ok(_) => return Err("Unexpected response type".to_string()),
        Err(e) => return Err(e),
    };

    // Count windows per workspace, excluding ignored windows
    let mut window_counts: HashMap<u64, usize> = HashMap::new();
    for window in windows {
        // Check if window should be ignored
        let should_ignore = ignore_rules.iter().any(|rule| {
            let app_id_matches = rule.app_id.as_ref().map_or(true, |app_id| {
                window.app_id.as_ref().map_or(false, |w_app_id| w_app_id == app_id)
            });
            let title_matches = rule.title.as_ref().map_or(true, |title| {
                window.title.as_ref().map_or(false, |w_title| w_title == title)
            });
            app_id_matches && title_matches
        });

        if !should_ignore {
            if let Some(ws_id) = window.workspace_id {
                *window_counts.entry(ws_id).or_insert(0) += 1;
            }
        }
    }

    Ok((workspaces, window_counts))
}

fn get_pie_icon(count: usize, size: Option<&str>) -> String {
    let size_attr = size.map(|s| format!(" size='{}'", s)).unwrap_or_default();

    // Add thin space after icon to center it (compensate for glyph asymmetry)
    match count {
        0 => format!("<span{}>󰋙\u{2009}</span>", size_attr),       // Empty hexagon
        1 => format!("<span{}>󰫃\u{2009}</span>", size_attr),       // 1/8 hexagon
        2 => format!("<span{}>󰫄\u{2009}</span>", size_attr),       // 2/8 hexagon
        3 => format!("<span{}>󰫅\u{2009}</span>", size_attr),       // 3/8 hexagon
        4 => format!("<span{}>󰫆\u{2009}</span>", size_attr),       // 4/8 hexagon
        5 => format!("<span{}>󰫇\u{2009}</span>", size_attr),       // 5/8 hexagon
        6 => format!("<span{}>󰫈\u{2009}</span>", size_attr),       // 6/8 hexagon (3/4 filled)
        7 => format!("<span foreground='#bf616a'{}>󰫈\u{2009}</span>", size_attr),   // Red
        8 => format!("<span foreground='#d08770'{}>󰫈\u{2009}</span>", size_attr),   // Orange
        9 => format!("<span foreground='#ebcb8b'{}>󰫈\u{2009}</span>", size_attr),   // Yellow
        10 => format!("<span foreground='#a3be8c'{}>󰫈\u{2009}</span>", size_attr),  // Green
        11 => format!("<span foreground='#81a1c1'{}>󰫈\u{2009}</span>", size_attr),  // Blue
        12 => format!("<span foreground='#b48ead'{}>󰫈\u{2009}</span>", size_attr),  // Purple
        13 => format!("<span foreground='#8b7355'{}>󰫈\u{2009}</span>", size_attr),  // Brown
        14 => format!("<span foreground='#808080'{}>󰫈\u{2009}</span>", size_attr),  // Grey
        _ => format!("<span foreground='#000000'{}>󰫈\u{2009}</span>", size_attr),   // Black (15+)
    }
}

fn get_format_icon(ws: &niri_ipc::Workspace, format_icons: Option<&FormatIcons>, value: &str) -> String {
    if let Some(icons) = format_icons {
        // Priority order: urgent > empty > focused > active > named > indexed > default
        if ws.is_urgent {
            if let Some(icon) = &icons.urgent {
                return icon.clone();
            }
        }

        if ws.active_window_id.is_none() {
            if let Some(icon) = &icons.empty {
                return icon.clone();
            }
        }

        if ws.is_focused {
            if let Some(icon) = &icons.focused {
                return icon.clone();
            }
        }

        if ws.is_active {
            if let Some(icon) = &icons.active {
                return icon.clone();
            }
        }

        // Check for named workspace icons
        if let Some(name) = &ws.name {
            if let Some(icon) = icons.named.get(name) {
                return icon.clone();
            }
        }

        // Check for index-based icons
        let idx_str = ws.idx.to_string();
        if let Some(icon) = icons.named.get(&idx_str) {
            return icon.clone();
        }

        // Fall back to default
        if let Some(icon) = &icons.default {
            return icon.clone();
        }
    }

    // Ultimate fallback: use the value
    value.to_string()
}

fn focus_workspace(id: u64) {
    if let Ok(mut socket) = Socket::connect() {
        let _ = socket.send(Request::Action(Action::FocusWorkspace {
            reference: WorkspaceReferenceArg::Id(id),
        }));
    }
}

fn setup_workspace_drag_drop(button: &gtk::Button, ws_id: u64) {
    use std::cell::RefCell;
    use std::rc::Rc;

    // Define drag targets - use a unique type for workspaces
    let drag_targets = vec![gtk::TargetEntry::new(
        "application/x-workspace",
        gtk::TargetFlags::SAME_APP,
        0,
    )];

    // Set up as drag source
    button.drag_source_set(
        gtk::gdk::ModifierType::BUTTON1_MASK,
        &drag_targets,
        gtk::gdk::DragAction::MOVE,
    );

    // Set up as drag destination
    button.drag_dest_set(
        gtk::DestDefaults::ALL,
        &drag_targets,
        gtk::gdk::DragAction::MOVE,
    );

    // Track the starting index
    let start_index = Rc::new(RefCell::new(0usize));
    let start_index_begin = start_index.clone();
    let start_index_end = start_index.clone();

    button.connect_drag_begin(move |widget, _| {
        // Store the starting index
        if let Some(parent) = widget.parent() {
            if let Ok(container) = parent.downcast::<gtk::Box>() {
                let pos = container.child_position(widget);
                *start_index_begin.borrow_mut() = pos as usize;
            }
        }

        widget.style_context().add_class("dragging");
    });

    // Send workspace ID on drag
    button.connect_drag_data_get(move |_, _, data, _, _| {
        data.set_text(&ws_id.to_string());
    });

    // Handle the actual move when drag ends
    let button_for_end = button.clone();
    button.connect_drag_end(move |widget, _| {
        button_for_end.style_context().remove_class("dragging");

        // Get the final position after visual reordering
        if let Some(parent) = widget.parent() {
            if let Ok(container) = parent.downcast::<gtk::Box>() {
                let final_pos = container.child_position(widget) as usize;
                let start_pos = *start_index_end.borrow();

                if final_pos != start_pos {
                    // Position in GTK maps to workspace index (1-based)
                    // Position 0 = workspace index 1, position 1 = workspace index 2, etc.
                    let target_idx = final_pos + 1;

                    let _ = move_workspace_to_index(ws_id, target_idx);
                }
            }
        }
    });

    // Reorder visually during drag motion
    button.connect_drag_motion(move |widget, ctx, _, _, _| {
        if let Some(source) = ctx.drag_get_source_widget() {
            if source != *widget {
                if let Some(parent) = widget.parent() {
                    if let Ok(container) = parent.downcast::<gtk::Box>() {
                        let source_pos = container.child_position(&source);
                        let target_pos = container.child_position(widget);

                        if source_pos != target_pos {
                            container.reorder_child(&source, target_pos);
                        }
                    }
                }
            }
        }
        true
    });
}

fn move_workspace_to_index(ws_id: u64, target_index: usize) -> Result<(), String> {
    // Get current workspaces to find which one is focused
    let mut socket = Socket::connect().map_err(|e| e.to_string())?;
    let reply = socket.send(Request::Workspaces).map_err(|e| e.to_string())?;

    let workspaces = match reply {
        Ok(Response::Workspaces(ws)) => ws,
        Ok(_) => return Err("Unexpected response type".to_string()),
        Err(e) => return Err(e),
    };

    let currently_focused = workspaces.iter().find(|w| w.is_focused).map(|w| w.id);

    // Use MoveWorkspaceToIndex action to move the workspace directly
    let mut socket = Socket::connect().map_err(|e| e.to_string())?;
    let reply = socket.send(Request::Action(Action::MoveWorkspaceToIndex {
        index: target_index,
        reference: Some(WorkspaceReferenceArg::Id(ws_id)),
    })).map_err(|e| e.to_string())?;

    match reply {
        Ok(Response::Handled) => {},
        Ok(_) => return Err("Failed to move workspace".to_string()),
        Err(e) => return Err(e),
    }

    // Restore original focused workspace if it was different
    if let Some(original_focused) = currently_focused {
        if original_focused != ws_id {
            let mut socket = Socket::connect().map_err(|e| e.to_string())?;
            let _ = socket.send(Request::Action(Action::FocusWorkspace {
                reference: WorkspaceReferenceArg::Id(original_focused),
            }));
        }
    }

    Ok(())
}

waybar_module!(NiriWorkspaces);
