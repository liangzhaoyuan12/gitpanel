/// Embedded license templates — compiled into the binary from text/*.txt.
pub struct LicenseTemplate {
    pub name: &'static str,
    pub content: &'static str,
}

include!(concat!(env!("OUT_DIR"), "/licenses.rs"));
