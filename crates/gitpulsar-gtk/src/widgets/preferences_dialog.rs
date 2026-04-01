use adw::prelude::*;

use crate::config::{AppConfig, DateFormat};

pub fn build_preferences_dialog<F>(config: &AppConfig, on_changed: F) -> adw::PreferencesDialog
where
    F: Fn(AppConfig) + Clone + 'static,
{
    let dialog = adw::PreferencesDialog::new();

    // === General page ===
    let page = adw::PreferencesPage::builder()
        .title("General")
        .icon_name("preferences-system-symbolic")
        .build();

    // --- Display group ---
    let display_group = adw::PreferencesGroup::builder()
        .title("Display")
        .build();

    // Date format combo
    let date_format_model = gtk::StringList::new(&[
        &format!("European ({})", DateFormat::European.label()),
        &format!("ISO ({})", DateFormat::Iso.label()),
        &format!("American ({})", DateFormat::American.label()),
    ]);
    let date_row = adw::ComboRow::builder()
        .title("Date Format")
        .model(&date_format_model)
        .selected(config.date_format.index())
        .build();
    display_group.add(&date_row);

    // Commit files limit
    let files_limit_adj = gtk::Adjustment::new(
        config.commit_files_limit as f64,
        0.0,
        100.0,
        1.0,
        5.0,
        0.0,
    );
    let files_limit_row = adw::SpinRow::builder()
        .title("Commit files limit")
        .subtitle("0 = show all files")
        .adjustment(&files_limit_adj)
        .build();
    display_group.add(&files_limit_row);

    // Sidebar items limit
    let sidebar_limit_adj = gtk::Adjustment::new(
        config.sidebar_items_limit as f64,
        0.0,
        50.0,
        1.0,
        5.0,
        0.0,
    );
    let sidebar_limit_row = adw::SpinRow::builder()
        .title("Sidebar items limit")
        .subtitle("0 = show all items per section")
        .adjustment(&sidebar_limit_adj)
        .build();
    display_group.add(&sidebar_limit_row);

    page.add(&display_group);

    // --- Updates group ---
    let updates_group = adw::PreferencesGroup::builder()
        .title("Updates")
        .build();

    // Refresh interval spin
    let adj = gtk::Adjustment::new(
        config.refresh_interval_secs as f64,
        0.0,
        60.0,
        5.0,
        5.0,
        0.0,
    );
    let spin_row = adw::SpinRow::builder()
        .title("Auto-refresh interval (seconds)")
        .subtitle("0 = disabled")
        .adjustment(&adj)
        .build();
    updates_group.add(&spin_row);

    // Refresh Now button
    let refresh_row = adw::ActionRow::builder()
        .title("Refresh Now")
        .subtitle("Force an immediate refresh")
        .activatable(true)
        .build();
    let refresh_icon = gtk::Image::from_icon_name("view-refresh-symbolic");
    refresh_row.add_suffix(&refresh_icon);
    updates_group.add(&refresh_row);
    page.add(&updates_group);

    dialog.add(&page);

    // --- Signals ---
    // We build the new config from current widget values on each change
    let build_config = {
        let date_row = date_row.clone();
        let spin_row = spin_row.clone();
        let files_limit_row = files_limit_row.clone();
        let sidebar_limit_row = sidebar_limit_row.clone();
        let recent = config.recent_workspaces.clone();
        move || AppConfig {
            date_format: DateFormat::from_index(date_row.selected()),
            refresh_interval_secs: spin_row.value() as u32,
            commit_files_limit: files_limit_row.value() as u32,
            sidebar_items_limit: sidebar_limit_row.value() as u32,
            recent_workspaces: recent.clone(),
        }
    };

    {
        let on_changed = on_changed.clone();
        let build_config = build_config.clone();
        date_row.connect_selected_notify(move |_| {
            on_changed(build_config());
        });
    }

    {
        let on_changed = on_changed.clone();
        let build_config = build_config.clone();
        spin_row.connect_value_notify(move |_| {
            on_changed(build_config());
        });
    }

    {
        let on_changed = on_changed.clone();
        let build_config = build_config.clone();
        files_limit_row.connect_value_notify(move |_| {
            on_changed(build_config());
        });
    }

    {
        let on_changed = on_changed.clone();
        let build_config = build_config.clone();
        sidebar_limit_row.connect_value_notify(move |_| {
            on_changed(build_config());
        });
    }

    // Refresh Now — emit special config with refresh_interval_secs = u32::MAX as signal
    {
        let on_changed = on_changed;
        let build_config = build_config;
        refresh_row.connect_activated(move |_| {
            let mut cfg = build_config();
            // Use a sentinel value to signal "refresh now"
            cfg.refresh_interval_secs = u32::MAX;
            on_changed(cfg);
        });
    }

    dialog
}
