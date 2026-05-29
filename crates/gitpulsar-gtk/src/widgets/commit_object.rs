//! GObject wrapper for a single commit entry used as the model item for the
//! virtualized commits ListView. Mirrors widgets/changed_file_object.rs.

use std::cell::{Cell, RefCell};

use adw::subclass::prelude::*;
use gtk::glib;

use gitpulsar_core::models::{CommitInfo, DiffFile, Signature};

mod imp {
    use super::*;

    pub struct CommitObject {
        pub id: RefCell<String>,
        pub short_id: RefCell<String>,
        pub summary: RefCell<String>,
        pub message: RefCell<String>,
        pub author: RefCell<Signature>,
        pub time_unix: Cell<i64>,
        pub parent_ids: RefCell<Vec<String>>,
        pub tags: RefCell<Vec<String>>,
        pub files: RefCell<Vec<DiffFile>>,
        pub is_signed: Cell<bool>,
        pub is_head: Cell<bool>,
        pub is_unpushed: Cell<bool>,
        pub expanded: Cell<bool>,
        pub files_loaded: Cell<bool>,
        pub is_load_more_sentinel: Cell<bool>,
    }

    impl Default for CommitObject {
        fn default() -> Self {
            Self {
                id: RefCell::new(String::new()),
                short_id: RefCell::new(String::new()),
                summary: RefCell::new(String::new()),
                message: RefCell::new(String::new()),
                author: RefCell::new(Signature {
                    name: String::new(),
                    email: String::new(),
                }),
                time_unix: Cell::new(0),
                parent_ids: RefCell::new(Vec::new()),
                tags: RefCell::new(Vec::new()),
                files: RefCell::new(Vec::new()),
                is_signed: Cell::new(false),
                is_head: Cell::new(false),
                is_unpushed: Cell::new(false),
                expanded: Cell::new(false),
                files_loaded: Cell::new(false),
                is_load_more_sentinel: Cell::new(false),
            }
        }
    }

    #[glib::object_subclass]
    impl ObjectSubclass for CommitObject {
        const NAME: &'static str = "GpCommitObject";
        type Type = super::CommitObject;
        type ParentType = glib::Object;
    }

    impl ObjectImpl for CommitObject {}
}

glib::wrapper! {
    pub struct CommitObject(ObjectSubclass<imp::CommitObject>);
}

impl CommitObject {
    pub fn from_info(
        info: &CommitInfo,
        tags: Vec<String>,
        is_head: bool,
        is_unpushed: bool,
    ) -> Self {
        let obj: Self = glib::Object::new();
        let imp = obj.imp();
        *imp.id.borrow_mut() = info.id.clone();
        *imp.short_id.borrow_mut() = info.short_id.clone();
        *imp.summary.borrow_mut() = info.summary.clone();
        *imp.message.borrow_mut() = info.message.clone();
        *imp.author.borrow_mut() = info.author.clone();
        imp.time_unix.set(info.time.timestamp());
        *imp.parent_ids.borrow_mut() = info.parent_ids.clone();
        *imp.tags.borrow_mut() = tags;
        imp.is_signed.set(info.is_signed);
        imp.is_head.set(is_head);
        imp.is_unpushed.set(is_unpushed);
        obj
    }

    pub fn load_more_sentinel() -> Self {
        let obj: Self = glib::Object::new();
        obj.imp().is_load_more_sentinel.set(true);
        obj
    }

    pub fn id(&self) -> String {
        self.imp().id.borrow().clone()
    }
    pub fn short_id(&self) -> String {
        self.imp().short_id.borrow().clone()
    }
    pub fn summary(&self) -> String {
        self.imp().summary.borrow().clone()
    }
    pub fn message(&self) -> String {
        self.imp().message.borrow().clone()
    }
    pub fn author(&self) -> Signature {
        self.imp().author.borrow().clone()
    }
    pub fn time_unix(&self) -> i64 {
        self.imp().time_unix.get()
    }
    pub fn parent_ids(&self) -> Vec<String> {
        self.imp().parent_ids.borrow().clone()
    }
    pub fn tags(&self) -> Vec<String> {
        self.imp().tags.borrow().clone()
    }
    pub fn files(&self) -> Vec<DiffFile> {
        self.imp().files.borrow().clone()
    }
    pub fn set_files(&self, files: Vec<DiffFile>) {
        *self.imp().files.borrow_mut() = files;
    }

    pub fn is_signed(&self) -> bool {
        self.imp().is_signed.get()
    }
    pub fn set_is_signed(&self, v: bool) {
        self.imp().is_signed.set(v);
    }
    pub fn is_head(&self) -> bool {
        self.imp().is_head.get()
    }
    pub fn is_unpushed(&self) -> bool {
        self.imp().is_unpushed.get()
    }
    pub fn expanded(&self) -> bool {
        self.imp().expanded.get()
    }
    pub fn set_expanded(&self, v: bool) {
        self.imp().expanded.set(v);
    }
    pub fn files_loaded(&self) -> bool {
        self.imp().files_loaded.get()
    }
    pub fn set_files_loaded(&self, v: bool) {
        self.imp().files_loaded.set(v);
    }
    pub fn is_load_more_sentinel(&self) -> bool {
        self.imp().is_load_more_sentinel.get()
    }
}

impl Default for CommitObject {
    fn default() -> Self {
        glib::Object::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use chrono::TimeZone;
    use serial_test::serial;

    use gitpulsar_core::models::{CommitInfo, Signature};

    use crate::test_support;

    fn sample_info(idx: u32) -> CommitInfo {
        CommitInfo {
            id: format!("{:040x}", idx),
            short_id: format!("{:07x}", idx),
            summary: format!("commit {idx}"),
            message: format!("commit {idx}\n\nbody"),
            author: Signature {
                name: "Test".into(),
                email: "t@e.com".into(),
            },
            committer: Signature {
                name: "Test".into(),
                email: "t@e.com".into(),
            },
            time: chrono::Utc
                .timestamp_opt(1_700_000_000 + idx as i64, 0)
                .single()
                .expect("valid timestamp"),
            parent_ids: if idx == 0 {
                vec![]
            } else {
                vec![format!("{:040x}", idx - 1)]
            },
            is_signed: false,
        }
    }

    #[test]
    #[serial]
    fn from_info_copies_fields() {
        test_support::ensure_gtk_init();
        if !test_support::gtk_available() {
            return;
        }

        let info = sample_info(7);
        let obj = CommitObject::from_info(&info, vec!["v1".into()], true, false);

        assert_eq!(obj.id(), info.id);
        assert_eq!(obj.short_id(), info.short_id);
        assert_eq!(obj.summary(), info.summary);
        assert_eq!(obj.message(), info.message);
        assert_eq!(obj.tags(), vec!["v1".to_string()]);
        assert!(obj.is_head());
        assert!(!obj.is_unpushed());
        assert!(!obj.expanded());
        assert!(!obj.files_loaded());
        assert!(!obj.is_load_more_sentinel());
    }

    #[test]
    #[serial]
    fn load_more_sentinel_flag() {
        test_support::ensure_gtk_init();
        if !test_support::gtk_available() {
            return;
        }

        let s = CommitObject::load_more_sentinel();
        assert!(s.is_load_more_sentinel());
        assert!(s.id().is_empty());
        assert!(!s.is_head());
    }

    #[test]
    #[serial]
    fn expanded_toggle() {
        test_support::ensure_gtk_init();
        if !test_support::gtk_available() {
            return;
        }

        let obj = CommitObject::from_info(&sample_info(0), vec![], false, false);
        assert!(!obj.expanded());
        obj.set_expanded(true);
        assert!(obj.expanded());
        obj.set_expanded(false);
        assert!(!obj.expanded());
    }
}
