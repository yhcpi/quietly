use serde::{Deserialize, Serialize};
use tauri::menu::{IsMenuItem, Menu, MenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Manager, Runtime};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TrayAction {
    Show,
    Quit,
}

impl TrayAction {
    pub fn from_event_id(id: &str) -> Option<Self> {
        match id {
            "show" => Some(TrayAction::Show),
            "quit" => Some(TrayAction::Quit),
            _ => None,
        }
    }

    pub fn event_id(&self) -> &'static str {
        match self {
            TrayAction::Show => "show",
            TrayAction::Quit => "quit",
        }
    }
}

pub const MENU_ITEMS: &[(TrayAction, &str)] = &[
    (TrayAction::Show, "Show"),
    (TrayAction::Quit, "Quit"),
];

pub fn setup<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<()> {
    let menu_items: Vec<MenuItem<R>> = MENU_ITEMS
        .iter()
        .map(|(action, label)| {
            MenuItem::with_id(app, action.event_id(), *label, true, None::<&str>)
        })
        .collect::<tauri::Result<Vec<_>>>()?;

    let item_refs: Vec<&dyn IsMenuItem<R>> = menu_items
        .iter()
        .map(|i| i as &dyn IsMenuItem<R>)
        .collect();

    let menu = Menu::with_items(app, &item_refs)?;

    let _tray = TrayIconBuilder::with_id("main")
        .icon(app.default_window_icon().expect("no default icon").clone())
        .menu(&menu)
        .on_menu_event(|app, event| {
            if let Some(action) = TrayAction::from_event_id(event.id.as_ref()) {
                match action {
                    TrayAction::Show => {
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.show();
                            let _ = window.set_focus();
                        }
                    }
                    TrayAction::Quit => app.exit(0),
                }
            }
        })
        .build(app)?;
    Ok(())
}
