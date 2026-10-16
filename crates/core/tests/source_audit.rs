//! Source-level checks that Clippy cannot express. They read the Rust files
//! in `crates/core/src` and `src-tauri/src` and fail on patterns that could
//! leak a key.

#![allow(clippy::unwrap_used, clippy::panic)]

use std::path::{Path, PathBuf};

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            rust_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

fn sources() -> Vec<(PathBuf, String)> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut files = Vec::new();
    rust_files(&root.join("src"), &mut files);
    rust_files(&root.join("../../src-tauri/src"), &mut files);
    assert!(files.len() >= 10, "source audit found too few files: {files:?}");
    files
        .into_iter()
        .map(|p| {
            let text = std::fs::read_to_string(&p).unwrap();
            (p, text)
        })
        .collect()
}

/// Drop `#[cfg(test)] mod tests { ... }` blocks, which may print or unwrap.
fn without_tests(text: &str) -> &str {
    match text.find("#[cfg(test)]\nmod tests") {
        Some(i) => &text[..i],
        None => text,
    }
}

const SECRET_TYPES: &[&str] = &["SecretString", "SecretBox", "Zeroizing"];

#[test]
fn no_debug_derive_on_secret_bearing_types() {
    for (path, text) in sources() {
        let code = without_tests(&text);
        let lines: Vec<&str> = code.lines().collect();
        for (i, line) in lines.iter().enumerate() {
            let t = line.trim_start();
            let is_item = [
                "pub struct ",
                "struct ",
                "pub enum ",
                "enum ",
                "pub(crate) struct ",
                "pub(crate) enum ",
            ]
            .iter()
            .any(|k| t.starts_with(k));
            if !is_item {
                continue;
            }
            // Item body: until the matching closing brace at the same indent,
            // or the end of a tuple/unit struct line.
            let indent = line.len() - t.len();
            let mut body = String::from(*line);
            if !(t.ends_with(';') || t.contains(");")) {
                for next in &lines[i + 1..] {
                    body.push_str(next);
                    body.push('\n');
                    if next.len() - next.trim_start().len() == indent && next.trim_start().starts_with('}') {
                        break;
                    }
                }
            }
            if !SECRET_TYPES.iter().any(|s| body.contains(s)) {
                continue;
            }
            let mut j = i;
            while j > 0 {
                let prev = lines[j - 1].trim_start();
                if prev.starts_with("#[") || prev.starts_with("///") || prev.starts_with("//") {
                    assert!(
                        !(prev.starts_with("#[derive") && prev.contains("Debug")),
                        "{}:{}: secret-bearing type derives Debug",
                        path.display(),
                        i + 1
                    );
                    j -= 1;
                } else {
                    break;
                }
            }
        }
    }
}

#[test]
fn no_print_macros_outside_tests() {
    for (path, text) in sources() {
        for (n, line) in without_tests(&text).lines().enumerate() {
            let code = line.split("//").next().unwrap_or_default();
            for mac in ["println!", "eprintln!", "print!(", "eprint!(", "dbg!("] {
                assert!(!code.contains(mac), "{}:{}: `{mac}` outside tests", path.display(), n + 1);
            }
        }
    }
}

#[test]
fn exposed_secrets_never_reach_format_or_log_macros() {
    let macros = [
        "format!",
        "format_args!",
        "write!",
        "writeln!",
        "trace!",
        "debug!",
        "info!",
        "warn!",
        "error!",
        "panic!",
        "anyhow!",
    ];
    for (path, text) in sources() {
        for (n, line) in without_tests(&text).lines().enumerate() {
            if line.contains("expose_secret") {
                for m in macros {
                    assert!(!line.contains(m), "{}:{}: expose_secret() next to `{m}`", path.display(), n + 1);
                }
            }
        }
    }
}

#[test]
fn serpapi_body_has_no_echoed_identity_fields() {
    let text = std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("src/providers/serpapi.rs")).unwrap();
    let body_struct = text.split("struct Body {").nth(1).and_then(|rest| rest.split('}').next()).unwrap();
    for field in ["api_key", "account_email", "account_id"] {
        assert!(!body_struct.contains(field), "serpapi::Body must not deserialize `{field}`");
    }
}

#[test]
fn no_frontend_reachable_network_plugins() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../src-tauri");
    let manifest = std::fs::read_to_string(root.join("Cargo.toml")).unwrap();
    for plugin in [
        "tauri-plugin-http",
        "tauri-plugin-shell",
        "tauri-plugin-fs",
        "tauri-plugin-opener",
        "tauri-plugin-stronghold",
    ] {
        assert!(!manifest.contains(plugin), "src-tauri depends on {plugin}");
    }
    let conf = std::fs::read_to_string(root.join("tauri.conf.json")).unwrap();
    assert!(conf.contains("\"withGlobalTauri\": false"));
    assert!(conf.contains("\"connect-src\": \"'none'\""));
}
