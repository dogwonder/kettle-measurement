//! What counts as a network call site (#233).
//!
//! The scanner is deliberately dumb: a marker on a line of source that
//! is not a comment. A cleverer one — resolving aliases, following
//! calls — would be more precise and much harder to trust, and the
//! thing a reviewer needs from this crate is to believe its answer
//! without reading it.
//!
//! Two consequences of being dumb are worth stating, because they are
//! the shape of what it can miss:
//!
//! * It sees names, not behaviour. A dependency that opens its own
//!   socket from its own thread has no marker in *our* source, so the
//!   dependency inventory in `privacy-boundary.toml` — enabled features
//!   and all — is the part of the audit that covers it, and the
//!   packaged-build observation #233 also asks for is the part that
//!   covers what neither can see.
//! * It reads what ships. Test code is excluded, on the ground that it
//!   is compiled out (`#[cfg(test)]`) or built as a separate target
//!   (`crates/*/tests`, `*.test.ts`) and so cannot be present in a
//!   packaged release. Kettle's tests bind loopback sockets constantly
//!   to stand up mock model servers; declaring each one would bury the
//!   handful of entries that describe the shipped application under
//!   dozens that describe its test harness, and a boundary file nobody
//!   finishes reading protects nobody.

use crate::CallSite;
use std::path::{Path, PathBuf};

/// Rust APIs that can open a socket. `std::net` is here as well as the
/// concrete types so that an aliased import (`use std::net::TcpStream
/// as T`) still lands on the `use` line.
const RUST_MARKERS: &[&str] = &[
    "reqwest",
    "ureq",
    "TcpStream",
    "TcpListener",
    "UdpSocket",
    "std::net",
];

/// Browser APIs that can leave the page. `sendBeacon` is in the list
/// because it is the one an analytics snippet reaches for first.
const WEB_MARKERS: &[&str] = &[
    "fetch(",
    "XMLHttpRequest",
    "WebSocket",
    "EventSource",
    "sendBeacon",
];

/// Trees that are not Kettle's source. Every one of them is gitignored
/// — generated output, vendored binaries or scratch — bar the last two,
/// which need saying out loud:
///
/// `crates/privacy-audit` is the contract's exclusion, and this file is
/// where its own markers are written down. `reference/` holds design
/// mock-ups and learning guides that are never built and never bundled;
/// they load React from a CDN quite deliberately, and reporting that as
/// an application network path would teach a reviewer to skim the file.
///
/// `gen/` is worth naming too: `tauri-build` writes the capability
/// schemas there, and their documentation examples are full of
/// `https://mydomain.dev` — a boundary file explaining a placeholder in
/// a generated schema is a boundary file nobody believes.
const NOT_SOURCE: &[&str] = &[
    ".git",
    "target",
    "node_modules",
    "sidecars",
    "models",
    "gen",
    "dist",
    "dist-demo",
    "dist-study",
    "runs",
    "crates/privacy-audit",
    "reference",
];

/// How a file's comments are written, so a URL in prose is not read as
/// a URL the application fetches.
#[derive(Clone, Copy)]
struct Syntax {
    /// `//` to end of line, and `/* … */`.
    slashes: bool,
    /// `<!-- … -->`.
    html: bool,
    /// Characters that open a string literal, where comment markers
    /// stop counting.
    quotes: &'static str,
    /// Rust identifiers and `use` statements are worth matching.
    rust: bool,
    /// Browser globals are worth matching.
    web: bool,
}

fn syntax_for(path: &Path) -> Option<Syntax> {
    let base = Syntax {
        slashes: false,
        html: false,
        quotes: "\"",
        rust: false,
        web: false,
    };
    match path.extension()?.to_str()? {
        "rs" => Some(Syntax {
            slashes: true,
            rust: true,
            ..base
        }),
        // Svelte carries both markup and script; give it both comment
        // styles rather than guess which half a line came from.
        "svelte" => Some(Syntax {
            slashes: true,
            html: true,
            quotes: "\"'",
            web: true,
            ..base
        }),
        "ts" | "js" | "mjs" => Some(Syntax {
            slashes: true,
            quotes: "\"'`",
            web: true,
            ..base
        }),
        "scss" => Some(Syntax {
            slashes: true,
            quotes: "\"'",
            ..base
        }),
        // No `//` in CSS or HTML: a bare `https://…` in markup would be
        // eaten by a comment rule that does not exist there.
        "css" => Some(Syntax {
            quotes: "\"'",
            ..base
        }),
        "html" | "tera" => Some(Syntax {
            html: true,
            quotes: "\"'",
            ..base
        }),
        "json" => Some(base),
        _ => None,
    }
}

pub(crate) fn scan(root: &Path) -> Vec<CallSite> {
    let mut found = Vec::new();
    walk(root, root, &mut found);
    found.sort_by(|a, b| (&a.file, a.line).cmp(&(&b.file, b.line)));
    found
}

fn walk(root: &Path, dir: &Path, found: &mut Vec<CallSite>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Some(rel) = relative(root, &path) else {
            continue;
        };
        if NOT_SOURCE.iter().any(|skip| {
            rel == *skip
                || rel.starts_with(&format!("{skip}/"))
                || rel.ends_with(&format!("/{skip}"))
        }) {
            continue;
        }
        if path.is_dir() {
            if is_test_path(&rel) {
                continue;
            }
            walk(root, &path, found);
        } else if !is_test_path(&rel) {
            if let Some(syntax) = syntax_for(&path) {
                if let Ok(text) = std::fs::read_to_string(&path) {
                    sites_in(&rel, &text, syntax, found);
                }
            }
        }
    }
}

fn relative(root: &Path, path: &Path) -> Option<String> {
    Some(
        path.strip_prefix(root)
            .ok()?
            .to_string_lossy()
            .replace('\\', "/"),
    )
}

/// Test targets: a Cargo `tests/` directory, a vitest file, or the
/// harness that sets one up. Never linked into a shipped binary or
/// bundled by `vite build`.
fn is_test_path(rel: &str) -> bool {
    rel.split('/').any(|part| part == "tests")
        || rel.ends_with(".test.ts")
        || rel.ends_with(".test.js")
        || rel.ends_with("test-setup.ts")
}

fn sites_in(rel: &str, text: &str, syntax: Syntax, found: &mut Vec<CallSite>) {
    for (line_no, line) in clean_lines(text, syntax) {
        // One site per line, not per marker. `use std::net::{TcpListener,
        // TcpStream}` is one fact about a file, and splitting it into
        // three would cost three entries in the boundary file to say it.
        let mut calls: Vec<String> = Vec::new();
        if syntax.rust {
            calls.extend(
                RUST_MARKERS
                    .iter()
                    .filter(|m| line.contains(**m))
                    .map(|m| (*m).to_string()),
            );
        }
        if syntax.web {
            calls.extend(
                WEB_MARKERS
                    .iter()
                    .filter(|m| line.contains(**m))
                    .map(|m| (*m).to_string()),
            );
        }
        calls.extend(urls_in(&line));
        if !calls.is_empty() {
            found.push(CallSite {
                file: PathBuf::from(rel),
                line: line_no,
                calls: calls.join(", "),
            });
        }
    }
}

/// Every absolute http(s) URL on a line, minus the two shapes that only
/// look like one.
fn urls_in(line: &str) -> Vec<String> {
    let mut urls = Vec::new();
    let bytes = line.as_bytes();
    let mut from = 0;
    while let Some(offset) = line[from..].find("http") {
        let at = from + offset;
        from = at + 4;
        let rest = &line[at..];
        if !(rest.starts_with("http://") || rest.starts_with("https://")) {
            continue;
        }

        // A CSS attribute selector — `[href^="https://"]` — is a match
        // on a link's text, not a fetch. The operator before the `=` is
        // what tells them apart.
        let before = line[..at].trim_end();
        let before = before.strip_suffix(['"', '\'']).unwrap_or(before);
        if let Some(head) = before.strip_suffix('=') {
            if head
                .chars()
                .next_back()
                .is_some_and(|c| "^*$~|".contains(c))
            {
                continue;
            }
        }

        // `$schema` is a JSON Schema identifier. Nothing in Kettle
        // resolves one; editors use it to offer completion, and the
        // validator is configured not to fetch a `$ref` at all.
        if line.contains("$schema") {
            continue;
        }

        let end = rest
            .find(|c: char| c.is_whitespace() || "\"'`)<>,\\{".contains(c))
            .unwrap_or(rest.len());
        let url = rest[..end].trim_end_matches([':', '/', '?', '#']);
        if !url.is_empty() {
            urls.push(url.to_string());
        }
        from = at + end.max(1);
        if from >= bytes.len() {
            break;
        }
    }
    urls
}

/// The file with its comments removed and its `#[cfg(test)]` items
/// dropped, as `(line number, code)`.
fn clean_lines(text: &str, syntax: Syntax) -> Vec<(usize, String)> {
    let mut out = Vec::new();
    let mut in_block = false;
    let mut in_html = false;

    for (index, raw) in text.lines().enumerate() {
        let mut code = String::with_capacity(raw.len());
        let mut chars = raw.char_indices().peekable();
        let mut string: Option<char> = None;

        while let Some((i, c)) = chars.next() {
            if in_block {
                if c == '*' && chars.peek().is_some_and(|(_, n)| *n == '/') {
                    chars.next();
                    in_block = false;
                }
                continue;
            }
            if in_html {
                if raw[i..].starts_with("-->") {
                    chars.next();
                    chars.next();
                    in_html = false;
                }
                continue;
            }
            if let Some(quote) = string {
                code.push(c);
                if c == '\\' {
                    if let Some((_, escaped)) = chars.next() {
                        code.push(escaped);
                    }
                } else if c == quote {
                    string = None;
                }
                continue;
            }
            if syntax.html && raw[i..].starts_with("<!--") {
                in_html = true;
                continue;
            }
            if syntax.slashes && raw[i..].starts_with("/*") {
                in_block = true;
                chars.next();
                continue;
            }
            // `//` ends a line — unless it is the one in `https://`,
            // which is the only place a colon precedes it.
            if syntax.slashes && raw[i..].starts_with("//") && !code.ends_with(':') {
                break;
            }
            if syntax.quotes.contains(c) {
                string = Some(c);
            }
            code.push(c);
        }

        out.push((index + 1, code));
    }

    if syntax.rust {
        drop_cfg_test(&mut out);
    }
    out
}

/// Remove every `#[cfg(test)]` item, braces and all. Such code is not
/// in a release build, so a socket it opens is not one Kettle can open.
fn drop_cfg_test(lines: &mut Vec<(usize, String)>) {
    let mut keep = Vec::with_capacity(lines.len());
    let mut skipping = false;
    let mut depth = 0i32;
    let mut opened = false;

    for (number, code) in lines.iter() {
        if !skipping {
            let trimmed = code.trim();
            if trimmed.starts_with("#[cfg(")
                && !trimmed.contains("not(test")
                && (trimmed.contains("(test)") || trimmed.contains("(test,"))
            {
                skipping = true;
                depth = 0;
                opened = false;
                continue;
            }
            keep.push((*number, code.clone()));
            continue;
        }

        for c in code.chars() {
            match c {
                '{' => {
                    depth += 1;
                    opened = true;
                }
                '}' => depth -= 1,
                _ => {}
            }
        }
        // The item ends where its braces close — or, for a
        // `#[cfg(test)] use …;` that has no braces at all, at the
        // semicolon.
        let ended = if opened {
            depth <= 0
        } else {
            code.trim_end().ends_with(';')
        };
        if ended {
            skipping = false;
        }
    }

    *lines = keep;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn calls(name: &str, text: &str) -> Vec<String> {
        let mut found = Vec::new();
        let path = PathBuf::from(name);
        let syntax = syntax_for(&path).expect("a syntax for this extension");
        sites_in(name, text, syntax, &mut found);
        found.into_iter().map(|site| site.calls).collect()
    }

    /// Build output is not source (#269, #431). `app/dist-demo/` is the
    /// public demo's bundle and `app/dist-study/` the participant
    /// harness's: minified Svelte, whose error messages carry
    /// `https://svelte.dev/e/...` URLs, plus whatever the page itself
    /// links to. Reporting those as application network paths would
    /// declare the same call site twice — once where it is written and
    /// once where it was compiled — and the second copy is unreviewable.
    ///
    /// Pinned here rather than left to the boundary test, because CI's
    /// Rust job never builds the frontend: the directories do not exist
    /// there, so a regression only ever appears on a machine that has
    /// run `bun run demo:build` or `bun run study:build`. That is not
    /// hypothetical — the study harness's first build turned the
    /// boundary test red locally while CI would have stayed green.
    #[test]
    fn a_frontend_build_is_not_scanned() {
        for (built_dir, source_dir) in [
            ("app/dist-demo/assets", "app/demo"),
            ("app/dist-study/assets", "app/study"),
        ] {
            let root =
                std::env::temp_dir().join(format!("kettle-scan-{}", built_dir.replace('/', "-")));
            let _ = std::fs::remove_dir_all(&root);
            let built = root.join(built_dir);
            std::fs::create_dir_all(&built).expect("a temp tree");
            std::fs::write(built.join("index.js"), "fetch(\"https://example.com\")")
                .expect("a built bundle");

            assert_eq!(scan(&root), Vec::new(), "{built_dir} was scanned");

            let authored = root.join(source_dir);
            std::fs::create_dir_all(&authored).expect("a source dir");
            std::fs::write(
                authored.join("Site.svelte"),
                "fetch(\"https://example.com\")",
            )
            .expect("a source file");

            // The same line, authored rather than built, is still seen —
            // otherwise this test would pass on a scanner that walks
            // nothing.
            assert_eq!(scan(&root).len(), 1, "{source_dir} was not scanned");
            let _ = std::fs::remove_dir_all(&root);
        }
    }

    #[test]
    fn a_css_attribute_selector_is_not_a_fetch() {
        // The report template styles external links by matching their
        // href. Reading that as a network path would put a line in the
        // boundary file that describes a `::after` content rule.
        assert!(calls(
            "report.html.tera",
            r#"[href^="https://"].govuk-link::after { content: " (link)"; }"#,
        )
        .is_empty());
        assert_eq!(
            calls("index.html", r#"<link href="https://cdn.example/x.css">"#),
            vec!["https://cdn.example/x.css"],
        );
    }

    #[test]
    fn a_url_in_prose_is_not_a_url_the_app_fetches() {
        assert!(calls("dev.html", "<!-- open http://localhost:1420/ -->").is_empty());
        assert!(calls("mod.rs", "// weights come from https://example.test").is_empty());
        assert!(calls("mod.rs", "/* https://example.test */").is_empty());
        // …but the `//` in a URL must not be read as a comment.
        assert_eq!(
            calls("mod.rs", r#"let url = "http://127.0.0.1:8080";"#),
            vec!["http://127.0.0.1:8080"],
        );
    }

    #[test]
    fn a_json_schema_key_names_a_schema_rather_than_a_host() {
        // Nothing in Kettle resolves a `$schema`; editors use it for
        // completion. `tauri.conf.json` would otherwise declare a path
        // that is neither loopback nor a model download.
        assert!(calls(
            "tauri.conf.json",
            r#"  "$schema": "https://schema.tauri.app/config/2","#,
        )
        .is_empty());
    }

    #[test]
    fn test_only_rust_is_not_shipped_rust() {
        let text = "\
fn real() {}

#[cfg(test)]
mod tests {
    use std::net::TcpListener;
    fn mock() {
        TcpListener::bind(\"127.0.0.1:0\").unwrap();
    }
}

fn after() { let _ = ureq::get(\"http://127.0.0.1\"); }
";
        assert_eq!(calls("lib.rs", text), vec!["ureq, http://127.0.0.1"]);
    }

    #[test]
    fn one_line_is_one_site_however_many_markers_it_carries() {
        assert_eq!(
            calls("sidecar.rs", "use std::net::{TcpListener, TcpStream};"),
            vec!["TcpStream, TcpListener, std::net"],
        );
    }

    #[test]
    fn the_browser_apis_that_can_leave_the_page_are_all_seen() {
        for source in [
            "await fetch(url);",
            "new XMLHttpRequest();",
            "new WebSocket(url);",
            "new EventSource(url);",
            "navigator.sendBeacon(url);",
        ] {
            assert!(
                !calls("api.ts", source).is_empty(),
                "missed a call site in {source}"
            );
        }
    }
}
