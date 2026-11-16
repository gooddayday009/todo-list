//! 系统托盘:左键单击切换岛显隐,右键菜单(显隐 / 停靠 / 自启 / 设置 / 退出)。

use tauri::menu::{CheckMenuItem, Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, Wry};
use tauri_plugin_autostart::ManagerExt;

use crate::island;

/// 托盘菜单里的"开机自启"勾选项句柄(切换后同步打勾状态)
pub struct TrayAutostartItem(pub CheckMenuItem<Wry>);

pub fn autostart_enabled(app: &AppHandle) -> bool {
    app.autolaunch().is_enabled().unwrap_or(false)
}

/// 设置命令改了自启后,同步托盘菜单勾选状态
pub fn sync_autostart_checked(app: &AppHandle, checked: bool) {
    if let Some(item) = app.try_state::<TrayAutostartItem>() {
        let _ = item.0.set_checked(checked);
    }
}

pub fn setup(app: &AppHandle) -> tauri::Result<()> {
    let toggle = MenuItem::with_id(
        app,
        "toggle",
        "显示 / 隐藏灵动岛 (Alt+Q)",
        true,
        None::<&str>,
    )?;
    let dock_left = MenuItem::with_id(app, "dock_left", "停靠到屏幕左缘", true, None::<&str>)?;
    let dock_right = MenuItem::with_id(app, "dock_right", "停靠到屏幕右缘", true, None::<&str>)?;
    let autostart = CheckMenuItem::with_id(
        app,
        "autostart",
        "开机自启",
        true,
        autostart_enabled(app),
        None::<&str>,
    )?;
    let settings = MenuItem::with_id(app, "settings", "设置", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "退出 weldd", true, None::<&str>)?;

    let menu = Menu::with_items(
        app,
        &[&toggle, &dock_left, &dock_right, &autostart, &settings, &quit],
    )?;

    app.manage(TrayAutostartItem(autostart));

    TrayIconBuilder::with_id("weldd-tray")
        .icon(
            app.default_window_icon()
                .expect("缺少应用图标")
                .clone(),
        )
        .tooltip("weldd · 灵动岛")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "toggle" => island::toggle_visible(app),
            "dock_left" => {
                if let Err(e) = island::set_mode_str(app, "docked-left") {
                    eprintln!("[weldd] 停靠左缘失败: {e}");
                }
            }
            "dock_right" => {
                if let Err(e) = island::set_mode_str(app, "docked-right") {
                    eprintln!("[weldd] 停靠右缘失败: {e}");
                }
            }
            "autostart" => {
                let next = !autostart_enabled(app);
                let result = if next {
                    app.autolaunch().enable()
                } else {
                    app.autolaunch().disable()
                };
                match result {
                    Ok(()) => sync_autostart_checked(app, next),
                    Err(e) => eprintln!("[weldd] 切换开机自启失败: {e}"),
                }
            }
            "settings" => island::open_settings(app),
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                island::toggle_visible(tray.app_handle());
            }
        })
        .build(app)?;
    Ok(())
}
