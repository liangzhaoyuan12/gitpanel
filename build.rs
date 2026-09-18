use std::env;
use std::fs;
use std::path::Path;

fn main() {
    let out_dir = env::var("OUT_DIR").unwrap();
    let text_dir = Path::new("text");

    // Collect all .txt files
    let mut entries: Vec<(String, String)> = Vec::new();
    if let Ok(dir) = fs::read_dir(text_dir) {
        for entry in dir.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("txt") {
                let name = path
                    .file_stem()
                    .unwrap()
                    .to_string_lossy()
                    .to_string();
                let content = fs::read_to_string(&path).unwrap_or_default();
                entries.push((name, content));
            }
        }
    }
    entries.sort_by(|a, b| a.0.cmp(&b.0));

    // Generate the licenses source file — only the function, struct is in the module file
    let mut out = String::new();
    out.push_str("#[allow(dead_code)]\n");
    out.push_str("pub fn all_licenses() -> Vec<LicenseTemplate> {\n");
    out.push_str("    vec![\n");

    for (name, content) in &entries {
        // Use r##"..."## to tolerate any `"#` sequences inside license text
        out.push_str(&format!(
            "        LicenseTemplate {{ name: \"{}\", content: r##\"{}\"## }},\n",
            name.replace('\\', "\\\\").replace('"', "\\\""),
            content
        ));
    }

    out.push_str("    ]\n");
    out.push_str("}\n");

    let dest_path = Path::new(&out_dir).join("licenses.rs");
    fs::write(&dest_path, out).unwrap();
}
