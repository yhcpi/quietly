use quietly_lib::tray::{TrayAction, MENU_ITEMS};

#[test]
fn from_event_id_maps_known_ids() {
    assert_eq!(TrayAction::from_event_id("show"), Some(TrayAction::Show));
    assert_eq!(TrayAction::from_event_id("quit"), Some(TrayAction::Quit));
}

#[test]
fn from_event_id_unknown_returns_none() {
    assert_eq!(TrayAction::from_event_id(""), None);
    assert_eq!(TrayAction::from_event_id("Show"), None); // case-sensitive
    assert_eq!(TrayAction::from_event_id("hide"), None);
    assert_eq!(TrayAction::from_event_id("pause"), None);
    assert_eq!(TrayAction::from_event_id("show "), None); // trailing space
}

#[test]
fn event_id_round_trip() {
    for action in [TrayAction::Show, TrayAction::Quit] {
        assert_eq!(TrayAction::from_event_id(action.event_id()), Some(action));
    }
}

#[test]
fn event_id_returns_distinct_non_empty_strings() {
    assert_ne!(TrayAction::Show.event_id(), TrayAction::Quit.event_id());
    assert!(!TrayAction::Show.event_id().is_empty());
    assert!(!TrayAction::Quit.event_id().is_empty());
}

#[test]
fn menu_items_contains_expected_actions_with_unique_event_ids() {
    let ids: Vec<&str> = MENU_ITEMS.iter().map(|(a, _)| a.event_id()).collect();
    let mut deduped = ids.clone();
    deduped.sort();
    deduped.dedup();
    assert_eq!(ids.len(), deduped.len(), "MENU_ITEMS event ids must be unique");
    assert!(MENU_ITEMS.iter().any(|(a, _)| *a == TrayAction::Show));
    assert!(MENU_ITEMS.iter().any(|(a, _)| *a == TrayAction::Quit));
}

#[test]
fn menu_item_labels_are_non_empty() {
    for (_, label) in MENU_ITEMS {
        assert!(!label.is_empty(), "menu item label must not be empty");
    }
}
