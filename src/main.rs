mod scanner;

use scanner::{PortEntry, SocketState};

use eframe::egui;
use log::{info, warn};

fn main() -> eframe::Result<()> {
    env_logger::init();
    info!("PortKill starting...");

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1200.0, 800.0])
            .with_resizable(true),
        ..Default::default()
    };

    eframe::run_native(
        "PortKill - Port Manager",
        options,
        Box::new(|cc| {
            // Apply dark theme
            apply_custom_theme(&cc.egui_ctx);
            Ok(Box::new(App::default()))
        }),
    )
}

fn apply_custom_theme(ctx: &egui::Context) {
    let mut style = (*ctx.style()).clone();

    // Dark theme colors based on design system
    let bg_primary = egui::Color32::from_rgb(15, 23, 42);      // #0F172A - slate-900
    let bg_secondary = egui::Color32::from_rgb(30, 41, 59);    // #1E293B - slate-800
    let bg_card = egui::Color32::from_rgb(51, 65, 85);         // #334155 - slate-700
    let text_primary = egui::Color32::from_rgb(248, 250, 252); // #F8FAFC - slate-50
    let text_muted = egui::Color32::from_rgb(148, 163, 184);   // #94A3B8 - slate-400
    let accent = egui::Color32::from_rgb(34, 197, 94);         // #22C55E - green-500
    let danger = egui::Color32::from_rgb(239, 68, 68);         // #EF4444 - red-500

    // Apply colors
    style.visuals.dark_mode = true;
    style.visuals.window_fill = bg_primary;
    style.visuals.panel_fill = bg_primary;
    style.visuals.faint_bg_color = bg_secondary;
    style.visuals.extreme_bg_color = bg_card;

    style.visuals.widgets.noninteractive.bg_fill = bg_secondary;
    style.visuals.widgets.noninteractive.fg_stroke.color = text_primary;

    style.visuals.widgets.inactive.bg_fill = bg_secondary;
    style.visuals.widgets.inactive.fg_stroke.color = text_muted;

    style.visuals.widgets.hovered.bg_fill = bg_card;
    style.visuals.widgets.hovered.fg_stroke.color = text_primary;

    style.visuals.widgets.active.bg_fill = accent;
    style.visuals.widgets.active.fg_stroke.color = text_primary;

    // Button styling (removed - rounding is handled differently in egui 0.31)

    ctx.set_style(style);
}

struct App {
    port_entries: Vec<PortEntry>,
    filter_text: String,
    show_listening: bool,
    show_established: bool,
    show_all: bool,
    auto_refresh: bool,
    refresh_interval: std::time::Duration,
    last_refresh: std::time::Instant,
    toasts: Vec<Toast>,
    confirmation_dialog: Option<Confirmation>,
    is_loading: bool,
}

impl Default for App {
    fn default() -> Self {
        let mut app = Self {
            port_entries: Vec::new(),
            filter_text: String::new(),
            show_listening: true,
            show_established: false,
            show_all: false,
            auto_refresh: false,
            refresh_interval: std::time::Duration::from_secs(5),
            last_refresh: std::time::Instant::now(),
            toasts: Vec::new(),
            confirmation_dialog: None,
            is_loading: false,
        };
        app.refresh();
        app
    }
}

impl App {
    fn refresh(&mut self) {
        self.last_refresh = std::time::Instant::now();
        self.is_loading = true;
        info!("Refreshing port list...");

        match scanner::Scanner::scan_tcp() {
            Ok(mut tcp_entries) => {
                match scanner::Scanner::scan_tcp6() {
                    Ok(mut tcp6_entries) => {
                        tcp_entries.append(&mut tcp6_entries);
                    }
                    Err(e) => {
                        warn!("Failed to scan TCP6: {}", e);
                    }
                }

                for entry in &mut tcp_entries {
                    scanner::ProcessInfo::map_pid_to_info(entry);
                }

                self.port_entries = tcp_entries;
                self.is_loading = false;
            }
            Err(e) => {
                warn!("Failed to scan TCP: {}", e);
                self.toasts.push(Toast {
                    message: format!("Failed to scan ports: {}", e),
                    timestamp: std::time::Instant::now(),
                    is_error: true,
                });
                self.is_loading = false;
            }
        }
    }

    fn apply_filters(&self) -> Vec<PortEntry> {
        self.port_entries
            .iter()
            .filter(|entry| {
                let port_match = entry.port.to_string().contains(&self.filter_text);
                let name_match = entry
                    .process_name
                    .to_lowercase()
                    .contains(&self.filter_text.to_lowercase());

                let text_match = port_match || name_match;

                if self.show_all {
                    text_match
                } else if self.show_listening {
                    entry.state == SocketState::Listen && text_match
                } else if self.show_established {
                    entry.state == SocketState::Established && text_match
                } else {
                    text_match
                }
            })
            .cloned()
            .collect()
    }

    fn show_confirmation(&mut self, entry: &PortEntry, use_sigkill: bool) {
        let signal = if use_sigkill {
            "SIGKILL (9)"
        } else {
            "SIGTERM (15)"
        };
        let message = format!(
            "Send {} to {} (PID {}) on port {}?",
            signal, entry.process_name, entry.pid.unwrap_or(0), entry.port
        );

        self.confirmation_dialog = Some(Confirmation {
            entry: entry.clone(),
            message,
            use_sigkill,
        });
    }

    fn confirm_kill(&mut self) {
        if let Some(confirmation) = self.confirmation_dialog.take() {
            match if confirmation.use_sigkill {
                scanner::Killer::kill_sigkill(confirmation.entry.pid.unwrap())
            } else {
                scanner::Killer::kill_sigterm(confirmation.entry.pid.unwrap())
            } {
                Ok(msg) => {
                    self.toasts.push(Toast {
                        message: msg,
                        timestamp: std::time::Instant::now(),
                        is_error: false,
                    });
                    // Refresh after kill
                    self.refresh();
                }
                Err(e) => {
                    self.toasts.push(Toast {
                        message: format!("Failed: {}", e),
                        timestamp: std::time::Instant::now(),
                        is_error: true,
                    });
                }
            }
        }
    }

    fn kill_entry(&mut self, entry: &PortEntry, use_sigkill: bool) {
        let pid = entry.pid.unwrap();

        if !scanner::Killer::can_kill(pid) {
            self.toasts.push(Toast {
                message: format!("Cannot kill system process (PID {})", pid),
                timestamp: std::time::Instant::now(),
                is_error: true,
            });
            return;
        }

        self.show_confirmation(entry, use_sigkill);
    }

    fn find_well_known_label(&self, port: u16) -> Option<&'static str> {
        match port {
            80 => Some("HTTP"),
            443 => Some("HTTPS"),
            3000 => Some("Node/React"),
            5432 => Some("PostgreSQL"),
            3306 => Some("MySQL"),
            6379 => Some("Redis"),
            27017 => Some("MongoDB"),
            8080 => Some("Alt HTTP"),
            5000 => Some("Flask/Django"),
            9000 => Some("PHP-FPM"),
            _ => None,
        }
    }

    fn get_state_color(&self, state: &SocketState) -> egui::Color32 {
        match state {
            SocketState::Listen => egui::Color32::from_rgb(34, 197, 94),      // green-500
            SocketState::Established => egui::Color32::from_rgb(234, 179, 8), // yellow-500
            SocketState::TimeWait => egui::Color32::from_rgb(148, 163, 184),  // slate-400
            SocketState::CloseWait => egui::Color32::from_rgb(251, 146, 60),  // orange-400
            _ => egui::Color32::from_rgb(148, 163, 184),                      // slate-400
        }
    }

    fn handle_keyboard_shortcuts(&mut self, ctx: &egui::Context) {
        // Ctrl/Cmd + R: Refresh
        if ctx.input(|i| i.modifiers.command && i.key_pressed(egui::Key::R)) {
            self.refresh();
        }

        // Ctrl/Cmd + F: Focus search
        if ctx.input(|i| i.modifiers.command && i.key_pressed(egui::Key::F)) {
            ctx.memory_mut(|mem| mem.request_focus(egui::Id::new("search_box")));
        }

        // Escape: Clear search or cancel dialog
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            if self.confirmation_dialog.is_some() {
                self.confirmation_dialog = None;
            } else if !self.filter_text.is_empty() {
                self.filter_text.clear();
            }
        }
    }
}

#[derive(Clone)]
struct Toast {
    message: String,
    timestamp: std::time::Instant,
    is_error: bool,
}

#[derive(Clone)]
struct Confirmation {
    entry: PortEntry,
    message: String,
    use_sigkill: bool,
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Handle keyboard shortcuts
        self.handle_keyboard_shortcuts(ctx);

        // Auto-refresh
        if self.auto_refresh && self.last_refresh.elapsed() >= self.refresh_interval {
            self.refresh();
        }

        egui::CentralPanel::default().show(ctx, |ui| {
            let filtered = self.apply_filters();

            // Header
            ui.add_space(16.0);
            ui.horizontal(|ui| {
                ui.heading(egui::RichText::new("PortKill").size(28.0));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(
                        egui::RichText::new(format!("Ports: {}", self.port_entries.len()))
                            .color(egui::Color32::from_rgb(148, 163, 184))
                    );
                });
            });

            ui.add_space(8.0);
            ui.separator();
            ui.add_space(16.0);

            // Search and controls
            egui::Frame::none()
                .inner_margin(12.0)
                .fill(egui::Color32::from_rgb(30, 41, 59))
                .rounding(8.0)
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label("🔍");
                        let search_response = ui.add_sized(
                            [350.0, 24.0],
                            egui::TextEdit::singleline(&mut self.filter_text)
                                .hint_text("Search by port number or process name...")
                                .id(egui::Id::new("search_box")),
                        );

                        if search_response.has_focus() {
                            ui.label(egui::RichText::new("Esc to clear")
                                .color(egui::Color32::from_rgb(148, 163, 184))
                                .size(12.0));
                        }

                        ui.add_space(8.0);

                        let refresh_btn = ui.add_sized(
                            [80.0, 24.0],
                            egui::Button::new("↻ Refresh")
                        ).on_hover_text("Ctrl+R to refresh");
                        if refresh_btn.clicked() {
                            self.refresh();
                        }

                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.checkbox(&mut self.auto_refresh, "Auto-refresh (5s)");
                        });
                    });
                });

            ui.add_space(12.0);

            // Filters
            egui::Frame::none()
                .inner_margin(12.0)
                .fill(egui::Color32::from_rgb(30, 41, 59))
                .rounding(8.0)
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new("Filter:").color(egui::Color32::from_rgb(148, 163, 184)));

                        if ui.radio(self.show_listening, "LISTEN").clicked() {
                            self.show_listening = true;
                            self.show_established = false;
                            self.show_all = false;
                        }

                        if ui.radio(self.show_established, "ESTABLISHED").clicked() {
                            self.show_listening = false;
                            self.show_established = true;
                            self.show_all = false;
                        }

                        if ui.radio(self.show_all, "ALL").clicked() {
                            self.show_listening = false;
                            self.show_established = false;
                            self.show_all = true;
                        }

                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if self.is_loading {
                                ui.spinner();
                                ui.label("Scanning...");
                            }
                        });
                    });
                });

            ui.add_space(16.0);

            // Table
            if filtered.is_empty() {
                ui.add_space(40.0);
                ui.vertical_centered(|ui| {
                    ui.label(egui::RichText::new("No ports found")
                        .size(18.0)
                        .color(egui::Color32::from_rgb(148, 163, 184)));
                    ui.add_space(8.0);
                    if !self.filter_text.is_empty() || self.show_listening || self.show_established {
                        ui.label("Try adjusting your filters or search terms");
                        ui.add_space(12.0);
                        if ui.button("Clear Filters").clicked() {
                            self.filter_text.clear();
                            self.show_listening = true;
                            self.show_established = false;
                            self.show_all = false;
                        }
                    }
                });
            } else {
                // Table header
                egui::Frame::none()
                    .inner_margin(egui::vec2(12.0, 8.0))
                    .fill(egui::Color32::from_rgb(51, 65, 85))
                    .rounding(6.0)
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(egui::RichText::new("PORT").strong().size(13.0))
                                .on_hover_text("Port number");
                            ui.add_space(20.0);

                            ui.label(egui::RichText::new("PROTOCOL").strong().size(13.0))
                                .on_hover_text("TCP or TCP6");
                            ui.add_space(20.0);

                            ui.label(egui::RichText::new("STATE").strong().size(13.0))
                                .on_hover_text("Socket state");
                            ui.add_space(30.0);

                            ui.label(egui::RichText::new("PID").strong().size(13.0))
                                .on_hover_text("Process ID");
                            ui.add_space(20.0);

                            ui.label(egui::RichText::new("PROCESS").strong().size(13.0))
                                .on_hover_text("Process name");
                            ui.add_space(90.0);

                            ui.label(egui::RichText::new("USER").strong().size(13.0))
                                .on_hover_text("Process owner");
                            ui.add_space(40.0);

                            ui.label(egui::RichText::new("ACTIONS").strong().size(13.0));
                        });
                    });

                ui.add_space(8.0);

                // Table rows
                egui::ScrollArea::vertical()
                    .max_height(ui.available_height() - 60.0)
                    .show(ui, |ui| {
                        for entry in &filtered {
                            let well_known = self.find_well_known_label(entry.port);
                            let state_color = self.get_state_color(&entry.state);

                            egui::Frame::none()
                                .inner_margin(egui::vec2(12.0, 10.0))
                                .fill(egui::Color32::from_rgb(30, 41, 59))
                                .rounding(6.0)
                                .show(ui, |ui| {
                                    ui.horizontal(|ui| {
                                        // Port
                                        ui.label(
                                            egui::RichText::new(format!("{}", entry.port))
                                                .size(14.0)
                                                .color(egui::Color32::from_rgb(34, 197, 94))
                                        );
                                        if let Some(label) = well_known {
                                            ui.label(
                                                egui::RichText::new(format!("({})", label))
                                                    .size(12.0)
                                                    .color(egui::Color32::from_rgb(148, 163, 184))
                                            );
                                        }
                                        ui.add_space(20.0);

                                        // Protocol
                                        ui.label(format!("{}", entry.protocol));
                                        ui.add_space(20.0);

                                        // State with color indicator
                                        ui.label(egui::RichText::new("●").color(state_color));
                                        ui.label(format!("{}", entry.state));
                                        ui.add_space(20.0);

                                        // PID
                                        ui.label(format!("{}", entry.pid.unwrap_or(0)));
                                        ui.add_space(20.0);

                                        // Process
                                        ui.label(
                                            egui::RichText::new(&entry.process_name)
                                                .color(egui::Color32::from_rgb(248, 250, 252))
                                        );
                                        ui.add_space(20.0);

                                        // User
                                        ui.label(
                                            egui::RichText::new(&entry.user)
                                                .color(egui::Color32::from_rgb(148, 163, 184))
                                        );

                                        // Actions (right-aligned)
                                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                            let can_kill = scanner::Killer::can_kill(entry.pid.unwrap());

                                            if can_kill {
                                                // SIGKILL button
                                                let sigkill_btn = ui.add_sized(
                                                    [75.0, 28.0],
                                                    egui::Button::new("SIGKILL")
                                                        .fill(egui::Color32::from_rgb(127, 29, 29))
                                                );
                                                if sigkill_btn.clicked() {
                                                    self.kill_entry(entry, true);
                                                }

                                                ui.add_space(6.0);

                                                // SIGTERM button
                                                let kill_btn = ui.add_sized(
                                                    [70.0, 28.0],
                                                    egui::Button::new("Kill")
                                                        .fill(egui::Color32::from_rgb(185, 28, 28))
                                                );
                                                if kill_btn.clicked() {
                                                    self.kill_entry(entry, false);
                                                }
                                            } else {
                                                ui.label(
                                                    egui::RichText::new("System Process")
                                                        .color(egui::Color32::from_rgb(100, 116, 139))
                                                        .size(12.0)
                                                );
                                            }
                                        });
                                    });
                                });

                            ui.add_space(6.0);
                        }
                    });

                ui.add_space(12.0);

                // Footer
                ui.separator();
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new(format!("Showing {} of {} ports", filtered.len(), self.port_entries.len()))
                            .color(egui::Color32::from_rgb(148, 163, 184))
                    );

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(
                            egui::RichText::new("Keyboard: Ctrl+R (Refresh) | Ctrl+F (Search) | Esc (Clear)")
                                .color(egui::Color32::from_rgb(100, 116, 139))
                                .size(11.0)
                        );
                    });
                });
            }
        });

        // Toast notifications
        let mut index = 0;
        while index < self.toasts.len() {
            let toast = &self.toasts[index];
            let duration = toast.timestamp.elapsed();

            if duration > std::time::Duration::from_secs(4) {
                self.toasts.remove(index);
            } else {
                let bg_color = if toast.is_error {
                    egui::Color32::from_rgb(127, 29, 29)
                } else {
                    egui::Color32::from_rgb(21, 128, 61)
                };

                egui::Area::new(egui::Id::new(format!("toast_{}", index)))
                    .anchor(egui::Align2::RIGHT_BOTTOM, [-20.0, -20.0 - (index as f32 * 60.0)])
                    .show(ctx, |ui| {
                        egui::Frame::none()
                            .fill(bg_color)
                            .rounding(8.0)
                            .inner_margin(egui::vec2(16.0, 12.0))
                            .show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    if toast.is_error {
                                        ui.label(egui::RichText::new("⚠").size(16.0));
                                    } else {
                                        ui.label(egui::RichText::new("✓").size(16.0));
                                    }
                                    ui.label(&toast.message);
                                });
                            });
                    });
                index += 1;
            }
        }

        // Confirmation dialog
        if self.confirmation_dialog.is_some() {
            let message = self.confirmation_dialog.as_ref().unwrap().message.clone();
            let mut should_cancel = false;
            let mut should_confirm = false;

            egui::Window::new("⚠ Confirm Action")
                .collapsible(false)
                .resizable(false)
                .fixed_size([450.0, 150.0])
                .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
                .show(ctx, |ui| {
                    ui.add_space(8.0);
                    ui.label(egui::RichText::new(&message).size(15.0));
                    ui.add_space(16.0);

                    ui.horizontal(|ui| {
                        if ui.add_sized([100.0, 32.0], egui::Button::new("Cancel")).clicked() {
                            should_cancel = true;
                        }

                        ui.add_space(8.0);

                        if ui.add_sized(
                            [100.0, 32.0],
                            egui::Button::new("Confirm")
                                .fill(egui::Color32::from_rgb(185, 28, 28))
                        ).clicked() {
                            should_confirm = true;
                        }
                    });
                });

            if should_cancel {
                self.confirmation_dialog = None;
            }
            if should_confirm {
                self.confirm_kill();
            }
        }

        // Request repaint for animations
        if !self.toasts.is_empty() || self.is_loading {
            ctx.request_repaint();
        }
    }
}
