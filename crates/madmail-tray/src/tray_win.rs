// Copyright (C) 2026 themadorg
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Windows system tray UI (tray-icon + winit event loop).

#![cfg(windows)]

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use tray_icon::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tray_icon::{Icon, TrayIconBuilder};
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::window::WindowId;

use crate::ops::{self, ServiceState};
use crate::paths;

const SERVICE_NAME: &str = "Madmail";

struct TrayApp {
    config: PathBuf,
    state_dir: PathBuf,
    madmail: PathBuf,
    tray: Option<tray_icon::TrayIcon>,
    item_status: MenuItem,
    item_open_admin: MenuItem,
    item_copy_token: MenuItem,
    item_start: MenuItem,
    item_stop: MenuItem,
    item_config: MenuItem,
    item_quit: MenuItem,
    exit: Arc<AtomicBool>,
}

impl TrayApp {
    fn refresh_status(&self) {
        let st = ops::query_service_state(SERVICE_NAME);
        let label = format!("Status: {}", st.as_str());
        self.item_status.set_text(label);
        let running = matches!(st, ServiceState::Running);
        self.item_start.set_enabled(!running);
        self.item_stop.set_enabled(running);
    }

    fn handle_menu(&mut self, id: &tray_icon::menu::MenuId) {
        if id == self.item_status.id() {
            self.refresh_status();
        } else if id == self.item_open_admin.id() {
            let url = paths::admin_url(&self.config);
            if let Err(e) = ops::open_url(&url) {
                eprintln!("open admin: {e}");
            }
        } else if id == self.item_copy_token.id() {
            if let Some(tok) = paths::read_admin_token(&self.state_dir) {
                // Best-effort: print and try clip.exe
                let _ = std::process::Command::new("cmd")
                    .args(["/C", "echo|set /p=", &tok, "|", "clip"])
                    .status();
                eprintln!("admin token (also attempted clipboard): {tok}");
            } else {
                eprintln!("admin token not found under {}", self.state_dir.display());
            }
        } else if id == self.item_start.id() {
            if let Err(e) =
                ops::run_madmail_service(&self.madmail, &self.config, &self.state_dir, "start")
            {
                eprintln!("start service: {e}");
            }
            self.refresh_status();
        } else if id == self.item_stop.id() {
            if let Err(e) =
                ops::run_madmail_service(&self.madmail, &self.config, &self.state_dir, "stop")
            {
                eprintln!("stop service: {e}");
            }
            self.refresh_status();
        } else if id == self.item_config.id() {
            let dir = self
                .config
                .parent()
                .map(|p| p.to_path_buf())
                .unwrap_or_else(|| self.state_dir.clone());
            if let Err(e) = ops::open_path(&dir) {
                eprintln!("open config: {e}");
            }
        } else if id == self.item_quit.id() {
            self.exit.store(true, Ordering::SeqCst);
        }
    }
}

impl ApplicationHandler for TrayApp {
    fn resumed(&mut self, _event_loop: &ActiveEventLoop) {}

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        if matches!(event, WindowEvent::CloseRequested) {
            event_loop.exit();
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        // Drain menu events.
        while let Ok(event) = MenuEvent::receiver().try_recv() {
            self.handle_menu(&event.id);
        }
        if self.exit.load(Ordering::SeqCst) {
            event_loop.exit();
        }
    }
}

fn default_icon() -> Icon {
    // 32x32 simple blue-ish RGBA square (no external assets).
    let size = 32u32;
    let mut rgba = vec![0u8; (size * size * 4) as usize];
    for px in rgba.chunks_exact_mut(4) {
        px[0] = 0x1e;
        px[1] = 0x88;
        px[2] = 0xe5;
        px[3] = 0xff;
    }
    Icon::from_rgba(rgba, size, size).expect("icon")
}

pub fn run(config: PathBuf, state_dir: PathBuf) -> Result<(), String> {
    let madmail = paths::madmail_binary();
    let item_status = MenuItem::new("Status: …", true, None);
    let item_open_admin = MenuItem::new("Open admin UI", true, None);
    let item_copy_token = MenuItem::new("Show / copy admin token", true, None);
    let item_start = MenuItem::new("Start service", true, None);
    let item_stop = MenuItem::new("Stop service", true, None);
    let item_config = MenuItem::new("Open config folder", true, None);
    let item_quit = MenuItem::new("Quit tray", true, None);

    let menu = Menu::new();
    menu.append(&item_status)
        .map_err(|e| format!("menu: {e}"))?;
    menu.append(&PredefinedMenuItem::separator())
        .map_err(|e| format!("menu: {e}"))?;
    menu.append(&item_open_admin)
        .map_err(|e| format!("menu: {e}"))?;
    menu.append(&item_copy_token)
        .map_err(|e| format!("menu: {e}"))?;
    menu.append(&PredefinedMenuItem::separator())
        .map_err(|e| format!("menu: {e}"))?;
    menu.append(&item_start).map_err(|e| format!("menu: {e}"))?;
    menu.append(&item_stop).map_err(|e| format!("menu: {e}"))?;
    menu.append(&PredefinedMenuItem::separator())
        .map_err(|e| format!("menu: {e}"))?;
    menu.append(&item_config)
        .map_err(|e| format!("menu: {e}"))?;
    menu.append(&item_quit).map_err(|e| format!("menu: {e}"))?;

    let tray = TrayIconBuilder::new()
        .with_menu(Box::new(menu))
        .with_tooltip("Madmail")
        .with_icon(default_icon())
        .build()
        .map_err(|e| format!("tray icon: {e}"))?;

    let mut app = TrayApp {
        config,
        state_dir,
        madmail,
        tray: Some(tray),
        item_status,
        item_open_admin,
        item_copy_token,
        item_start,
        item_stop,
        item_config,
        item_quit,
        exit: Arc::new(AtomicBool::new(false)),
    };
    app.refresh_status();

    let event_loop = EventLoop::new().map_err(|e| format!("event loop: {e}"))?;
    event_loop
        .run_app(&mut app)
        .map_err(|e| format!("event loop run: {e}"))?;
    Ok(())
}
