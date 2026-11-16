//! weldd —— Windows 灵动岛。Tauri 装配入口。

mod commands;
mod db;
mod island;
mod reminder;
mod tray;
mod webhook;

use tauri::Manager;
use tauri_plugin_global_shortcut::{Builder as ShortcutBuilder, ShortcutState};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let shortcuts = ShortcutBuilder::new()
        .with_shortcuts(["alt+q", "alt+n"])
        .expect("注册全局快捷键失败")
        .with_handler(|app, shortcut, event| {
            if event.state != ShortcutState::Pressed {
                return;
            }
            match shortcut.to_string().to_lowercase().as_str() {
                "alt+q" => island::toggle_visible(app),
                "alt+n" => island::quick_add(app),
                _ => {}
            }
        })
        .build();

    tauri::Builder::default()
        // 单实例:二次启动时唤起已有实例的岛
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            island::show_island(app);
        }))
        .plugin(shortcuts)
        .plugin(tauri_plugin_store::Builder::new().build())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .manage(island::IslandState::default())
        .setup(|app| {
            let database = db::init(app.handle())?;
            let db_path = database.path().to_path_buf();
            app.manage(database);

            island::init(app.handle())?;
            tray::setup(app.handle())?;

            let handle = app.handle().clone();
            if let Some(window) = app.get_webview_window("island") {
                let move_handle = handle.clone();
                window.on_window_event(move |event| {
                    if let tauri::WindowEvent::Moved(pos) = event {
                        island::on_moved(&move_handle, *pos);
                    }
                });
            }

            reminder::spawn(handle, db_path);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            island::get_island_mode,
            island::set_island_mode,
            island::drag_island,
            island::hide_island,
            webhook::get_webhook_config,
            webhook::save_webhook_config,
            webhook::test_webhook,
            commands::list_today_tasks,
            commands::list_month_stats,
            commands::list_day_tasks,
            commands::list_due_tasks,
            commands::close_reminder_dialog,
            commands::create_task,
            commands::update_task_title,
            commands::update_task_note,
            commands::set_task_status,
            commands::set_task_reminder,
            commands::delete_task,
            commands::get_autostart,
            commands::set_autostart,
            commands::reset_island_position,
            commands::open_island_settings,
            commands::get_ui_setting,
            commands::set_ui_setting,
            commands::export_tasks,
            commands::import_tasks,
        ])
        .run(tauri::generate_context!())
        .expect("weldd 启动失败");
}
