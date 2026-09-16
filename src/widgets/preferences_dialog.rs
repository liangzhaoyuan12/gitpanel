use adw::prelude::*;
use gtk::glib;
use std::cell::RefCell;
use std::rc::Rc;

use crate::i18n::{self, Key, Language};
use crate::utils::config::{AppConfig, DateFormat};
use crate::utils::external_editor::{self, DetectedEditor};

/// What a row of the "Open with" combo maps to.
#[derive(Clone, PartialEq)]
enum EditorChoice {
    Unset,
    Detected(String),
    Custom,
}

/// Rebuild the combo's model from the editors we currently know about, keeping
/// `current` selected. Called once synchronously and again when host detection
/// finishes, so the list may grow under the user without losing their choice.
fn fill_editor_combo(
    combo: &adw::ComboRow,
    choices: &Rc<RefCell<Vec<EditorChoice>>>,
    detected: &[DetectedEditor],
    current: Option<&str>,
) {
    let mut labels: Vec<String> = vec![i18n::t(Key::pref_not_configured).to_string()];
    let mut mapping: Vec<EditorChoice> = vec![EditorChoice::Unset];

    for editor in detected {
        labels.push(editor.name.clone());
        mapping.push(EditorChoice::Detected(editor.command.clone()));
    }

    labels.push(i18n::t(Key::pref_custom_command_option).to_string());
    mapping.push(EditorChoice::Custom);

    let selected = match current {
        None => 0,
        Some(cmd) => mapping
            .iter()
            .position(|c| matches!(c, EditorChoice::Detected(d) if d == cmd))
            .unwrap_or(mapping.len() - 1) as usize,
    };

    let refs: Vec<&str> = labels.iter().map(|s| s.as_str()).collect();
    combo.set_model(Some(&gtk::StringList::new(&refs)));
    *choices.borrow_mut() = mapping;
    combo.set_selected(selected as u32);
}

/// Always offer "Zed" in the picker, even when host detection does not find it.
/// Detection only lists editors it can resolve on PATH; the user explicitly wants
/// Zed selectable regardless, so we inject it (deduped against a detected copy).
fn ensure_zed_present(detected: &[DetectedEditor]) -> Vec<DetectedEditor> {
    if detected.iter().any(|d| d.name == "Zed") {
        return detected.to_vec();
    }
    let mut out = Vec::with_capacity(detected.len() + 1);
    out.push(DetectedEditor {
        name: "Zed".to_string(),
        command: "zed".to_string(),
    });
    out.extend(detected.iter().cloned());
    out
}

pub fn build_preferences_dialog<F>(config: &AppConfig, on_changed: F) -> adw::PreferencesDialog
where
    F: Fn(AppConfig) + Clone + 'static,
{
    let dialog = adw::PreferencesDialog::new();

    // === General page ===
    let page = adw::PreferencesPage::builder()
        .title(i18n::t(Key::pref_general))
        .icon_name("preferences-system-symbolic")
        .build();

    // --- Language group ---
    let lang_group = adw::PreferencesGroup::builder()
        .title(i18n::t(Key::pref_language))
        .build();
    let lang_model = gtk::StringList::new(&[
        i18n::t(Key::pref_lang_system),
        i18n::t(Key::pref_lang_zh_cn),
        i18n::t(Key::pref_lang_en),
    ]);
    let lang_row = adw::ComboRow::builder()
        .title(i18n::t(Key::pref_language))
        .model(&lang_model)
        .selected(config.language.index())
        .build();
    lang_group.add(&lang_row);
    page.add(&lang_group);

    // --- Display group ---
    let display_group = adw::PreferencesGroup::builder()
        .title(i18n::t(Key::pref_display))
        .build();

    // Date format combo
    let date_format_model = gtk::StringList::new(&[
        &format!("European ({})", DateFormat::European.label()),
        &format!("ISO ({})", DateFormat::Iso.label()),
        &format!("American ({})", DateFormat::American.label()),
    ]);
    let date_row = adw::ComboRow::builder()
        .title(i18n::t(Key::pref_date_format))
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
        .title(i18n::t(Key::pref_commit_files_limit))
        .subtitle(i18n::t(Key::pref_commit_files_limit_sub))
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
        .title(i18n::t(Key::pref_sidebar_items_limit))
        .subtitle(i18n::t(Key::pref_sidebar_items_limit_sub))
        .adjustment(&sidebar_limit_adj)
        .build();
    display_group.add(&sidebar_limit_row);

    page.add(&display_group);

    // --- Updates group ---
    let updates_group = adw::PreferencesGroup::builder()
        .title(i18n::t(Key::pref_updates))
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
        .title(i18n::t(Key::pref_auto_refresh))
        .subtitle(i18n::t(Key::pref_auto_refresh_sub))
        .adjustment(&adj)
        .build();
    updates_group.add(&spin_row);

    // Refresh Now button
    let refresh_row = adw::ActionRow::builder()
        .title(i18n::t(Key::pref_refresh_now))
        .subtitle(i18n::t(Key::pref_refresh_now_sub))
        .activatable(true)
        .build();
    let refresh_icon = gtk::Image::from_icon_name("view-refresh-symbolic");
    refresh_row.add_suffix(&refresh_icon);
    updates_group.add(&refresh_row);
    page.add(&updates_group);

    // --- External tools group ---
    let tools_group = adw::PreferencesGroup::builder()
        .title(i18n::t(Key::pref_external_tools))
        .description(i18n::t(Key::pref_external_tools_desc))
        .build();

    // The Flatpak build cannot see or start host applications, so there is
    // nothing to pick from — the desktop portal asks the host instead.
    let sandboxed = external_editor::in_flatpak();

    let editor_row = adw::ComboRow::builder()
        .title(i18n::t(Key::pref_open_with))
        .build();
    let custom_row = adw::EntryRow::builder()
        .title(i18n::t(Key::pref_custom_command))
        .show_apply_button(true)
        .build();
    custom_row.set_text(config.external_editor.as_deref().unwrap_or(""));

    let choices: Rc<RefCell<Vec<EditorChoice>>> = Rc::new(RefCell::new(Vec::new()));
    let refilling = Rc::new(std::cell::Cell::new(false));

    if sandboxed {
        tools_group.add(
            &adw::ActionRow::builder()
                .title(i18n::t(Key::pref_open_with))
                .subtitle(i18n::t(Key::pref_open_with_sub))
                .build(),
        );
    } else {
        tools_group.add(&editor_row);
        tools_group.add(&custom_row);
        let detected_initial = ensure_zed_present(&[]);
        fill_editor_combo(
            &editor_row,
            &choices,
            &detected_initial,
            config.external_editor.as_deref(),
        );
        custom_row.set_visible(matches!(
            choices.borrow().get(editor_row.selected() as usize),
            Some(EditorChoice::Custom)
        ));
    }

    page.add(&tools_group);

    dialog.add(&page);

    // Probing the host for installed editors costs a `flatpak-spawn` round
    // trip, so it runs off the UI thread and refills the combo on arrival.
    if !sandboxed {
        let (tx, rx) = async_channel::bounded::<Vec<DetectedEditor>>(1);
        std::thread::spawn(move || {
            let _ = tx.send_blocking(external_editor::detect_installed());
        });

        let editor_row = editor_row.clone();
        let choices = choices.clone();
        let current = config.external_editor.clone();
        let refilling = refilling.clone();
        glib::spawn_future_local(async move {
            let Ok(detected) = rx.recv().await else { return };
            if detected.is_empty() {
                return;
            }
            // `set_model` emits `selected` before the real selection is
            // restored; without this guard that transient 0 would be saved as
            // "Not configured" and wipe the user's editor.
            refilling.set(true);
            fill_editor_combo(
                &editor_row,
                &choices,
                &ensure_zed_present(&detected),
                current.as_deref(),
            );
            refilling.set(false);
        });

    }

    // --- Signals ---
    // We build the new config from current widget values on each change
    let build_config = {
        let date_row = date_row.clone();
        let spin_row = spin_row.clone();
        let files_limit_row = files_limit_row.clone();
        let sidebar_limit_row = sidebar_limit_row.clone();
        let editor_row = editor_row.clone();
        let custom_row = custom_row.clone();
        let choices = choices.clone();
        let lang_row = lang_row.clone();
        let recent = config.recent_workspaces.clone();
        // Sandboxed builds show no editor picker, so the combo sits at "Not
        // configured" and would otherwise wipe a command set outside Flatpak.
        let preserved_editor = config.external_editor.clone();
        move || AppConfig {
            date_format: DateFormat::from_index(date_row.selected()),
            refresh_interval_secs: spin_row.value() as u32,
            commit_files_limit: files_limit_row.value() as u32,
            sidebar_items_limit: sidebar_limit_row.value() as u32,
            recent_workspaces: recent.clone(),
            external_editor: if sandboxed {
                preserved_editor.clone()
            } else {
                match choices.borrow().get(editor_row.selected() as usize) {
                    Some(EditorChoice::Detected(cmd)) => Some(cmd.clone()),
                    Some(EditorChoice::Custom) => {
                        let text = custom_row.text().trim().to_string();
                        (!text.is_empty()).then_some(text)
                    }
                    _ => None,
                }
            },
            language: Language::from_index(lang_row.selected()),
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

    {
        let on_changed = on_changed.clone();
        let build_config = build_config.clone();
        lang_row.connect_selected_notify(move |_| {
            let cfg = build_config();
            i18n::set_language(cfg.language);
            on_changed(cfg);
        });
    }

    {
        let on_changed = on_changed.clone();
        let build_config = build_config.clone();
        let custom_row = custom_row.clone();
        let choices = choices.clone();
        let refilling = refilling.clone();
        editor_row.connect_selected_notify(move |row| {
            let is_custom = matches!(
                choices.borrow().get(row.selected() as usize),
                Some(EditorChoice::Custom)
            );
            custom_row.set_visible(is_custom);
            if refilling.get() {
                return;
            }
            on_changed(build_config());
        });
    }

    {
        let on_changed = on_changed.clone();
        let build_config = build_config.clone();
        custom_row.connect_apply(move |_| {
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

#[cfg(test)]
mod tests {
    use super::*;

    use serial_test::serial;

    use crate::test_support;

    /// Walk the dialog and collect the titles of every `AdwPreferencesGroup`.
    fn group_titles(dialog: &adw::PreferencesDialog) -> Vec<String> {
        let mut titles = Vec::new();
        let mut stack: Vec<gtk::Widget> = dialog.child().into_iter().collect();
        while let Some(widget) = stack.pop() {
            if let Ok(group) = widget.clone().downcast::<adw::PreferencesGroup>() {
                titles.push(group.title().to_string());
            }
            let mut child = widget.first_child();
            while let Some(w) = child {
                child = w.next_sibling();
                stack.push(w);
            }
        }
        titles
    }

    fn titles_with_sandbox(sandboxed: bool) -> Vec<String> {
        if sandboxed {
            std::env::set_var("GP_SIMULATE_FLATPAK", "1");
        } else {
            std::env::remove_var("GP_SIMULATE_FLATPAK");
        }
        let titles = test_support::on_gtk_thread(|| {
            let dialog = build_preferences_dialog(&AppConfig::default(), |_| {});
            group_titles(&dialog)
        });
        std::env::remove_var("GP_SIMULATE_FLATPAK");
        titles
    }

    #[test]
    #[serial]
    fn external_tools_group_is_offered_outside_the_sandbox() {
        test_support::ensure_gtk_init();
        if !test_support::gtk_available() {
            return;
        }
        assert!(titles_with_sandbox(false).contains(&i18n::t(crate::i18n::Key::pref_external_tools).to_string()));
    }

    /// Count `AdwEntryRow`s — the custom-command field is the only one in this
    /// dialog, so its presence stands in for "the editor picker is shown".
    fn entry_rows(dialog: &adw::PreferencesDialog) -> usize {
        let mut n = 0;
        let mut stack: Vec<gtk::Widget> = dialog.child().into_iter().collect();
        while let Some(widget) = stack.pop() {
            if widget.downcast_ref::<adw::EntryRow>().is_some() {
                n += 1;
            }
            let mut child = widget.first_child();
            while let Some(w) = child {
                child = w.next_sibling();
                stack.push(w);
            }
        }
        n
    }

    fn entry_rows_with_sandbox(sandboxed: bool) -> usize {
        if sandboxed {
            std::env::set_var("GP_SIMULATE_FLATPAK", "1");
        } else {
            std::env::remove_var("GP_SIMULATE_FLATPAK");
        }
        let n = test_support::on_gtk_thread(|| {
            entry_rows(&build_preferences_dialog(&AppConfig::default(), |_| {}))
        });
        std::env::remove_var("GP_SIMULATE_FLATPAK");
        n
    }

    #[test]
    #[serial]
    fn sandboxed_dialog_explains_instead_of_offering_a_picker() {
        test_support::ensure_gtk_init();
        if !test_support::gtk_available() {
            return;
        }
        // The group stays — the portal is still reachable from the menu — but
        // there is nothing to configure, so the custom-command field is gone.
        assert!(titles_with_sandbox(true).contains(&i18n::t(crate::i18n::Key::pref_external_tools).to_string()));
        assert_eq!(entry_rows_with_sandbox(true), 0);
        assert_eq!(entry_rows_with_sandbox(false), 1);
    }
}
