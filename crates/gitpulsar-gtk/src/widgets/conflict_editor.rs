use adw::prelude::*;

use gitpulsar_core::conflict::ConflictChunk;

/// Build a 3-way merge conflict editor dialog.
/// `chunks` are the parsed conflict regions.
/// `on_resolve` is called with the final merged content.
pub fn build_conflict_editor<F>(
    file_path: &str,
    chunks: &[ConflictChunk],
    on_resolve: F,
) -> adw::Window
where
    F: Fn(String) + 'static,
{
    let dialog = adw::Window::builder()
        .title(&format!("Resolve: {}", file_path))
        .default_width(900)
        .default_height(600)
        .build();

    let vbox = gtk::Box::new(gtk::Orientation::Vertical, 0);

    // Header bar
    let header = adw::HeaderBar::new();
    let resolve_btn = gtk::Button::builder()
        .label("Mark Resolved")
        .css_classes(["suggested-action"])
        .build();
    header.pack_end(&resolve_btn);
    vbox.append(&header);

    // Three-pane layout
    let panes = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    panes.set_vexpand(true);

    // Ours panel (read-only)
    let ours_view = create_text_panel("Ours", false);
    panes.append(&ours_view.0);

    panes.append(&gtk::Separator::new(gtk::Orientation::Vertical));

    // Result panel (editable)
    let result_view = create_text_panel("Result", true);
    panes.append(&result_view.0);

    panes.append(&gtk::Separator::new(gtk::Orientation::Vertical));

    // Theirs panel (read-only)
    let theirs_view = create_text_panel("Theirs", false);
    panes.append(&theirs_view.0);

    vbox.append(&panes);

    // Action buttons per conflict chunk
    let actions_bar = gtk::Box::new(gtk::Orientation::Horizontal, 4);
    actions_bar.set_margin_start(8);
    actions_bar.set_margin_end(8);
    actions_bar.set_margin_top(4);
    actions_bar.set_margin_bottom(4);

    let conflict_count = chunks.iter().filter(|c| c.is_conflict).count();
    let info = gtk::Label::builder()
        .label(&format!("{} conflict(s)", conflict_count))
        .css_classes(["caption", "dim-label"])
        .hexpand(true)
        .xalign(0.0)
        .build();
    actions_bar.append(&info);

    let accept_ours_all = gtk::Button::builder()
        .label("Accept All Ours")
        .css_classes(["flat"])
        .build();
    actions_bar.append(&accept_ours_all);

    let accept_theirs_all = gtk::Button::builder()
        .label("Accept All Theirs")
        .css_classes(["flat"])
        .build();
    actions_bar.append(&accept_theirs_all);

    vbox.append(&actions_bar);

    // Populate panels
    let ours_buf = ours_view.1.buffer();
    let theirs_buf = theirs_view.1.buffer();
    let result_buf = result_view.1.buffer();

    let mut ours_text = String::new();
    let mut theirs_text = String::new();
    let mut result_text = String::new();

    for chunk in chunks {
        if chunk.is_conflict {
            let ours_section = chunk.ours.join("\n");
            let theirs_section = chunk.theirs.join("\n");

            ours_text.push_str(&ours_section);
            ours_text.push('\n');
            theirs_text.push_str(&theirs_section);
            theirs_text.push('\n');
            // Result starts with ours by default — user can edit
            result_text.push_str("<<<< CONFLICT — edit this section >>>>\n");
            result_text.push_str(&ours_section);
            result_text.push('\n');
        } else {
            let section = chunk.ours.join("\n");
            ours_text.push_str(&section);
            ours_text.push('\n');
            theirs_text.push_str(&section);
            theirs_text.push('\n');
            result_text.push_str(&section);
            result_text.push('\n');
        }
    }

    ours_buf.set_text(&ours_text);
    theirs_buf.set_text(&theirs_text);
    result_buf.set_text(&result_text);

    // "Accept All Ours" — replace result with ours content
    {
        let result_buf = result_buf.clone();
        let chunks = chunks.to_vec();
        accept_ours_all.connect_clicked(move |_| {
            let text = build_resolved_text(&chunks, true);
            result_buf.set_text(&text);
        });
    }

    // "Accept All Theirs" — replace result with theirs content
    {
        let result_buf = result_buf.clone();
        let chunks = chunks.to_vec();
        accept_theirs_all.connect_clicked(move |_| {
            let text = build_resolved_text(&chunks, false);
            result_buf.set_text(&text);
        });
    }

    dialog.set_content(Some(&vbox));

    // Resolve handler
    {
        let result_tv = result_view.1.clone();
        let dialog_weak = dialog.downgrade();
        resolve_btn.connect_clicked(move |_| {
            let buf = result_tv.buffer();
            let text = buf.text(&buf.start_iter(), &buf.end_iter(), false);
            on_resolve(text.to_string());
            if let Some(d) = dialog_weak.upgrade() {
                d.close();
            }
        });
    }

    dialog
}

fn create_text_panel(title: &str, editable: bool) -> (gtk::Box, gtk::TextView) {
    let panel = gtk::Box::new(gtk::Orientation::Vertical, 0);
    panel.set_hexpand(true);

    let label = gtk::Label::builder()
        .label(title)
        .css_classes(["heading"])
        .margin_start(8)
        .margin_top(4)
        .margin_bottom(2)
        .build();
    panel.append(&label);

    let tv = gtk::TextView::builder()
        .editable(editable)
        .monospace(true)
        .left_margin(4)
        .right_margin(4)
        .top_margin(4)
        .bottom_margin(4)
        .vexpand(true)
        .wrap_mode(gtk::WrapMode::None)
        .build();

    if !editable {
        tv.add_css_class("dim-label");
    }

    let scrolled = gtk::ScrolledWindow::builder()
        .child(&tv)
        .vexpand(true)
        .build();
    panel.append(&scrolled);

    (panel, tv)
}

fn build_resolved_text(chunks: &[ConflictChunk], use_ours: bool) -> String {
    let mut text = String::new();
    for chunk in chunks {
        let lines = if chunk.is_conflict {
            if use_ours { &chunk.ours } else { &chunk.theirs }
        } else {
            &chunk.ours
        };
        text.push_str(&lines.join("\n"));
        text.push('\n');
    }
    text
}
