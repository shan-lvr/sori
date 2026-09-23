pub mod claude_cli;
mod commands;
mod controller;
pub mod local_stt;
mod platform;
mod recorder;
mod selftest;

use std::sync::Arc;

use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{Emitter, Manager, RunEvent, WindowEvent};

use controller::App;

pub fn run() {
    let mut builder = tauri::Builder::default();
    #[cfg(desktop)]
    {
        builder = builder.plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            show_main(app);
        }));
    }
    let app = builder
        .plugin(
            tauri_plugin_log::Builder::new()
                .clear_targets()
                .level(log::LevelFilter::Info)
                .target(tauri_plugin_log::Target::new(tauri_plugin_log::TargetKind::LogDir { file_name: Some("sori".into()) }))
                .target(tauri_plugin_log::Target::new(tauri_plugin_log::TargetKind::Stdout))
                .max_file_size(2_000_000)
                .build(),
        )
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_autostart::init(tauri_plugin_autostart::MacosLauncher::LaunchAgent, None))
        .setup(|app| {
            let handle = app.handle().clone();
            let data_dir = app.path().app_data_dir()?;
            let config_path = app.path().app_config_dir()?.join("settings.json");
            let state = App::new(handle.clone(), data_dir, config_path)?;
            app.manage(state.clone());

            // Overlays float above everything and never take focus.
            for label in ["hud", "card"] {
                if let Some(w) = app.get_webview_window(label) {
                    platform::configure_overlay(&w);
                }
            }

            // Global shortcuts.
            let (tx, rx) = std::sync::mpsc::channel();
            platform::start_hotkeys(state.engine.clone(), tx);
            state.run_hotkey_loop(rx);

            #[cfg(unix)]
            {
                // Dev hook: `kill -USR1 $(pgrep -x Sori)` toggles dictation, USR2 translation.
                use signal_hook::consts::{SIGUSR1, SIGUSR2};
                let st = state.clone();
                if let Ok(mut signals) = signal_hook::iterator::Signals::new([SIGUSR1, SIGUSR2]) {
                    std::thread::spawn(move || {
                        for sig in signals.forever() {
                            let action = if sig == SIGUSR1 { sori_core::hotkey::Action::Dictate } else { sori_core::hotkey::Action::Translate };
                            st.debug_toggle(action);
                        }
                    });
                }
            }

            if let Ok(wav) = std::env::var("SORI_SELFTEST") {
                let st = state.clone();
                tauri::async_runtime::spawn(selftest::run(st, wav));
            }

            commands::apply_settings(&state, None);
            state.prune_history();
            state.warm_up();
            build_tray(app)?;

            // First launch (or permissions missing): open the main window.
            let s = state.settings.read().clone();
            let perms = platform::permissions();
            if !s.onboarding_done || !perms.accessibility || s.show_in_dock {
                show_main(&handle);
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                if window.label() == "main" {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_settings,
            commands::save_settings,
            commands::reset_api_key,
            commands::test_api_keys,
            commands::get_permissions,
            commands::request_accessibility,
            commands::open_privacy_pane,
            commands::list_microphones,
            commands::start_mic_test,
            commands::stop_mic_test,
            commands::list_history,
            commands::delete_history,
            commands::delete_all_history,
            commands::retry_history,
            commands::export_audio,
            commands::copy_text,
            commands::list_dictionary,
            commands::add_words,
            commands::update_word,
            commands::delete_words,
            commands::import_dictionary,
            commands::get_stats,
            commands::local_models,
            commands::download_local_model,
            commands::pause_local_download,
            commands::cancel_local_download,
            commands::delete_local_model,
            commands::hud_stop,
            commands::hud_cancel,
            commands::hud_set_target,
            commands::card_close,
            commands::record_shortcut_start,
            commands::record_shortcut_cancel,
            commands::app_info,
            commands::process_text_preview,
            commands::claude_status,
            commands::claude_install,
            commands::claude_login,
            commands::claude_login_code,
            commands::claude_login_cancel,
            commands::claude_test,
        ])
        .build(tauri::generate_context!())
        .expect("error while building Sori");

    app.run(|handle, event| match event {
        #[cfg(target_os = "macos")]
        RunEvent::Reopen { .. } => show_main(handle),
        RunEvent::Exit => {
            if let Some(state) = handle.try_state::<Arc<App>>() {
                commands::restore_fn_usage(&state);
            }
        }
        _ => {}
    });
}

pub fn show_main(handle: &tauri::AppHandle) {
    if let Some(w) = handle.get_webview_window("main") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
}

fn tray_menu<R: tauri::Runtime, M: Manager<R>>(app: &M) -> tauri::Result<Menu<R>> {
    let s = app.try_state::<Arc<App>>().map(|a| a.settings.read().clone()).unwrap_or_default();
    let tr = |en, ko| controller::tr(&s, en, ko).to_string();
    let open = MenuItem::with_id(app, "open", tr("Open Sori", "Sori 열기"), true, None::<&str>)?;
    let settings = MenuItem::with_id(app, "settings", tr("Settings…", "설정…"), true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", tr("Quit Sori", "Sori 종료"), true, None::<&str>)?;
    Menu::with_items(app, &[&open, &settings, &PredefinedMenuItem::separator(app)?, &quit])
}

/// Re-label the tray menu after the interface language changes.
pub fn refresh_tray(handle: &tauri::AppHandle) {
    if let (Some(tray), Ok(menu)) = (handle.tray_by_id("sori"), tray_menu(handle)) {
        let _ = tray.set_menu(Some(menu));
    }
}

fn build_tray(app: &tauri::App) -> tauri::Result<()> {
    let menu = tray_menu(app)?;
    let icon = tauri::image::Image::from_bytes(include_bytes!("../icons/tray.png"))?;
    TrayIconBuilder::with_id("sori")
        .icon(icon)
        .icon_as_template(true)
        .tooltip("Sori")
        .menu(&menu)
        .show_menu_on_left_click(true)
        .on_menu_event(|app, e| match e.id.as_ref() {
            "open" => show_main(app),
            "settings" => {
                show_main(app);
                let _ = app.emit_to("main", "open-settings", ());
            }
            "quit" => app.exit(0),
            _ => {}
        })
        .build(app)?;
    Ok(())
}
