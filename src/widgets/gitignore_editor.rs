use adw::prelude::*;

/// Build a .gitignore editor dialog.
/// `content` is the current .gitignore text.
/// `on_save` is called with the new content when the user clicks Save.
pub fn build_gitignore_editor<F>(content: &str, on_save: F) -> adw::Dialog
where
    F: Fn(String) + 'static,
{
    let dialog = adw::Dialog::builder()
        .title(".gitignore")
        .content_width(500)
        .content_height(450)
        .build();

    let toolbar_view = adw::ToolbarView::new();

    // Header bar with Save button
    let header = adw::HeaderBar::new();
    let save_btn = gtk::Button::builder()
        .label("Save")
        .css_classes(["suggested-action"])
        .build();
    header.pack_end(&save_btn);
    toolbar_view.add_top_bar(&header);

    // Text editor
    let text_view = gtk::TextView::builder()
        .monospace(true)
        .wrap_mode(gtk::WrapMode::Word)
        .top_margin(8)
        .bottom_margin(8)
        .left_margin(8)
        .right_margin(8)
        .vexpand(true)
        .build();
    text_view.buffer().set_text(content);

    let scrolled = gtk::ScrolledWindow::builder()
        .child(&text_view)
        .vexpand(true)
        .build();
    toolbar_view.set_content(Some(&scrolled));

    dialog.set_child(Some(&toolbar_view));

    // Save handler
    {
        let text_view = text_view.clone();
        let dialog_weak = dialog.downgrade();
        save_btn.connect_clicked(move |_| {
            let buf = text_view.buffer();
            let text = buf.text(&buf.start_iter(), &buf.end_iter(), false);
            on_save(text.to_string());
            if let Some(d) = dialog_weak.upgrade() {
                d.close();
            }
        });
    }

    dialog
}
