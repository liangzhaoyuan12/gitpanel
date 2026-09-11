//! GObject wrapper for a single changed-file entry used as the model item for
//! the virtualized changes ListView.

use std::cell::{Cell, RefCell};

use adw::subclass::prelude::*;
use gtk::glib;

use crate::model::FileStatusKind;

mod imp {
    use super::*;

    #[derive(Default)]
    pub struct ChangedFileObject {
        pub path: RefCell<String>,
        pub status: Cell<FileStatusKind>,
        pub is_staged: Cell<bool>,
        pub expanded: Cell<bool>,
        pub diff_loaded: Cell<bool>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for ChangedFileObject {
        const NAME: &'static str = "GpChangedFileObject";
        type Type = super::ChangedFileObject;
        type ParentType = glib::Object;
    }

    impl ObjectImpl for ChangedFileObject {}
}

glib::wrapper! {
    pub struct ChangedFileObject(ObjectSubclass<imp::ChangedFileObject>);
}

impl ChangedFileObject {
    pub fn new(path: String, status: FileStatusKind, is_staged: bool) -> Self {
        let obj: Self = glib::Object::new();
        *obj.imp().path.borrow_mut() = path;
        obj.imp().status.set(status);
        obj.imp().is_staged.set(is_staged);
        obj
    }

    pub fn path(&self) -> String {
        self.imp().path.borrow().clone()
    }

    pub fn status(&self) -> FileStatusKind {
        self.imp().status.get()
    }

    pub fn is_staged(&self) -> bool {
        self.imp().is_staged.get()
    }

    pub fn expanded(&self) -> bool {
        self.imp().expanded.get()
    }

    pub fn set_expanded(&self, v: bool) {
        self.imp().expanded.set(v);
    }

    pub fn diff_loaded(&self) -> bool {
        self.imp().diff_loaded.get()
    }

    pub fn set_diff_loaded(&self, v: bool) {
        self.imp().diff_loaded.set(v);
    }
}

impl Default for ChangedFileObject {
    fn default() -> Self {
        Self::new(String::new(), FileStatusKind::New, false)
    }
}
