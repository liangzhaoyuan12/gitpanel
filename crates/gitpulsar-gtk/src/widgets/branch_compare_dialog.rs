use adw::glib;
use adw::prelude::*;

use gitpulsar_core::models::BranchInfo;

/// Build a branch compare dialog.
/// `branches` lists local and remote refs to choose from.
/// `on_compare` is called with (base, target) when the user clicks Compare.
/// The caller is expected to call `set_results` on the returned handle to populate the file list.
pub fn build_branch_compare_dialog<F>(
    branches: &[BranchInfo],
    initial_base: Option<&str>,
    initial_target: Option<&str>,
    on_compare: F,
) -> (adw::Dialog, BranchCompareRefs)
where
    F: Fn(String, String) + 'static,
{
    let dialog = adw::Dialog::builder()
        .title("Compare Branches")
        .content_width(720)
        .content_height(560)
        .build();

    let toolbar_view = adw::ToolbarView::new();
    let header = adw::HeaderBar::new();
    toolbar_view.add_top_bar(&header);

    let content = gtk::Box::new(gtk::Orientation::Vertical, 12);
    content.set_margin_start(16);
    content.set_margin_end(16);
    content.set_margin_top(16);
    content.set_margin_bottom(16);

    let names: Vec<String> = branches.iter().map(|b| b.name.clone()).collect();
    let model = gtk::StringList::new(&names.iter().map(|s| s.as_str()).collect::<Vec<_>>());

    let pick_group = adw::PreferencesGroup::builder().title("Refs").build();

    let base_row = adw::ComboRow::builder()
        .title("Base (from)")
        .subtitle("Older state")
        .model(&model)
        .build();
    if let Some(b) = initial_base {
        if let Some(idx) = names.iter().position(|n| n == b) {
            base_row.set_selected(idx as u32);
        }
    }
    pick_group.add(&base_row);

    let target_row = adw::ComboRow::builder()
        .title("Target (to)")
        .subtitle("Newer state")
        .model(&model)
        .build();
    if let Some(t) = initial_target {
        if let Some(idx) = names.iter().position(|n| n == t) {
            target_row.set_selected(idx as u32);
        }
    }
    pick_group.add(&target_row);
    content.append(&pick_group);

    let compare_btn = gtk::Button::builder()
        .label("Compare")
        .css_classes(["suggested-action", "pill"])
        .halign(gtk::Align::End)
        .build();
    content.append(&compare_btn);

    let summary_label = gtk::Label::builder()
        .label("")
        .css_classes(["caption", "dim-label"])
        .xalign(0.0)
        .build();
    content.append(&summary_label);

    let results_box = gtk::ListBox::builder()
        .selection_mode(gtk::SelectionMode::None)
        .css_classes(["boxed-list"])
        .build();

    let scrolled = gtk::ScrolledWindow::builder()
        .child(&results_box)
        .vexpand(true)
        .build();
    content.append(&scrolled);

    toolbar_view.set_content(Some(&content));
    dialog.set_child(Some(&toolbar_view));

    let names_for_btn = names.clone();
    let base_row_clone = base_row.clone();
    let target_row_clone = target_row.clone();
    compare_btn.connect_clicked(move |_| {
        let b = names_for_btn.get(base_row_clone.selected() as usize).cloned().unwrap_or_default();
        let t = names_for_btn.get(target_row_clone.selected() as usize).cloned().unwrap_or_default();
        if !b.is_empty() && !t.is_empty() && b != t {
            on_compare(b, t);
        }
    });

    let refs = BranchCompareRefs {
        results_box,
        summary_label,
    };
    (dialog, refs)
}

pub struct BranchCompareRefs {
    pub results_box: gtk::ListBox,
    pub summary_label: gtk::Label,
}

impl BranchCompareRefs {
    /// Replace the results list with these files.
    pub fn set_results(&self, files: &[gitpulsar_core::models::DiffFile]) {
        while let Some(child) = self.results_box.first_child() {
            self.results_box.remove(&child);
        }

        let total_added: usize = files.iter().map(|f| f.stats.insertions).sum();
        let total_removed: usize = files.iter().map(|f| f.stats.deletions).sum();
        self.summary_label.set_label(&format!(
            "{} file(s), +{}/-{} lines",
            files.len(),
            total_added,
            total_removed,
        ));

        if files.is_empty() {
            let empty = gtk::Label::builder()
                .label("No differences")
                .css_classes(["dim-label"])
                .margin_top(16)
                .margin_bottom(16)
                .build();
            self.results_box.append(&empty);
            return;
        }

        for file in files {
            let row = adw::ActionRow::builder()
                .title(glib::markup_escape_text(&file.path).as_str())
                .subtitle(format!(
                    "+{} −{}",
                    file.stats.insertions, file.stats.deletions
                ))
                .build();
            self.results_box.append(&row);
        }
    }
}
