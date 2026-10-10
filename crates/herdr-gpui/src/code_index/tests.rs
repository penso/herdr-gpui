#![allow(clippy::unwrap_used)]
use super::*;

fn names(language: Language, text: &str) -> Vec<(String, Kind, u32)> {
    scan(language, text, 100)
        .into_iter()
        .map(|found| (found.name, found.kind, found.line))
        .collect()
}

fn found(name: &str, kind: Kind, line: u32) -> (String, Kind, u32) {
    (name.into(), kind, line)
}

#[test]
fn rust_items_and_methods_are_read_by_their_keywords() {
    let text = "\
use std::fmt;
pub(crate) struct Index {
    root: PathBuf,
}
impl<T: Clone> fmt::Display for Wrapper<T> where T: Copy {
    pub const fn new() -> Self {}
    async unsafe fn run(&self) {}
}
pub enum Kind { A }
macro_rules! bail {
const LIMIT: usize = 3;
// fn commented() {}
let fn_like = 1;
";
    assert_eq!(
        names(Language::Rust, text),
        [
            found("Index", Kind::Type, 2),
            found(
                "impl fmt::Display for Wrapper<T> where T: Copy",
                Kind::Implementation,
                5
            ),
            found("new", Kind::Method, 6),
            found("run", Kind::Method, 7),
            found("Kind", Kind::Type, 9),
            found("bail", Kind::Macro, 10),
            found("LIMIT", Kind::Constant, 11),
        ]
    );
}

#[test]
fn other_languages_read_their_own_definitions() {
    assert_eq!(
        names(
            Language::Python,
            "class Repo:\n    async def fetch(self):\n        pass\ndef main():\n"
        ),
        [
            found("Repo", Kind::Type, 1),
            found("fetch", Kind::Method, 2),
            found("main", Kind::Function, 4),
        ]
    );
    assert_eq!(
        names(
            Language::Ruby,
            "module Herd\n  class Pane\n    def self.open?\n    def close!\n"
        ),
        [
            found("Herd", Kind::Module, 1),
            found("Pane", Kind::Type, 2),
            found("open?", Kind::Method, 3),
            found("close!", Kind::Method, 4),
        ]
    );
    assert_eq!(
        names(
            Language::Swift,
            "@MainActor final class Window {\n    public class func make() {}\n    init(frame: CGRect) {}\n}\nextension Window: View {\n"
        ),
        [
            found("Window", Kind::Type, 1),
            found("make", Kind::Method, 2),
            found("init", Kind::Function, 3),
            found("extension Window: View", Kind::Implementation, 5),
        ]
    );
    assert_eq!(
        names(
            Language::Go,
            "type Server struct {\nfunc (s *Server) Run() error {\nfunc main() {\n"
        ),
        [
            found("Server", Kind::Type, 1),
            found("Run", Kind::Function, 2),
            found("main", Kind::Function, 3),
        ]
    );
    assert_eq!(
        names(
            Language::Script,
            "export default function App() {}\nexport const load = async (id) => {}\nconst LIMIT = 3\nexport interface Props {}\n"
        ),
        [
            found("App", Kind::Function, 1),
            found("load", Kind::Function, 2),
            found("Props", Kind::Type, 4),
        ]
    );
    assert_eq!(
        names(
            Language::Zig,
            "pub const Pane = struct {\nconst std = @import(\"std\");\npub fn main() void {\n"
        ),
        [
            found("Pane", Kind::Type, 1),
            found("main", Kind::Function, 3)
        ]
    );
}

#[test]
fn names_never_carry_direction_overrides() {
    assert_eq!(
        names(Language::Rust, "impl Pane\u{202e} for X {\n"),
        [found("impl Pane for X", Kind::Implementation, 1)]
    );
}

#[test]
fn languages_follow_the_extension() {
    assert_eq!(Language::of("src/main.rs"), Some(Language::Rust));
    assert_eq!(Language::of("app/models/pane.rb"), Some(Language::Ruby));
    assert_eq!(Language::of("Sources/App.swift"), Some(Language::Swift));
    assert_eq!(Language::of("web/app.tsx"), Some(Language::Script));
    assert_eq!(Language::of("README.md"), None);
    assert_eq!(Language::of("Makefile"), None);
}

#[test]
fn scanning_stops_at_its_limit_and_skips_minified_lines() {
    let text = "fn a() {}\n".repeat(10);
    assert_eq!(scan(Language::Rust, &text, 3).len(), 3);
    let long = format!("fn {}() {{}}\n", "x".repeat(2000));
    assert!(scan(Language::Rust, &long, 3).is_empty());
}

#[test]
fn a_listing_keeps_sorted_names_inside_the_checkout() {
    let listing =
        "src/b.rs\0src/a.rs\0../escape.rs\0/abs.rs\0bad\x1b.rs\0src/a.rs\0rs.\u{202e}exe\0";
    let listing = [listing.as_bytes(), b"\xff.rs\0"].concat();
    let index = Index::from_listing(Path::new("/repo"), &listing);
    assert_eq!(index.files(), ["src/a.rs", "src/b.rs"]);
    assert_eq!(index.path(1), Some(PathBuf::from("/repo/src/b.rs")));
    assert!(!index.truncated());
}

fn git(dir: &Path, args: &[&str]) {
    let status = std::process::Command::new("git")
        .args([
            "-c",
            "user.email=t@t",
            "-c",
            "user.name=t",
            "-c",
            "commit.gpgsign=false",
            "-C",
        ])
        .arg(dir)
        .args(args)
        .output()
        .unwrap();
    assert!(status.status.success(), "{status:?}");
}

#[test]
fn a_checkout_is_indexed_from_what_git_lists() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    git(root, &["init", "-q"]);
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::create_dir_all(root.join("target")).unwrap();
    std::fs::write(root.join(".gitignore"), "target/\n").unwrap();
    std::fs::write(root.join("src/lib.rs"), "pub fn tracked() {}\n").unwrap();
    std::fs::write(root.join("src/new.py"), "def untracked():\n").unwrap();
    std::fs::write(root.join("target/gen.rs"), "fn ignored() {}\n").unwrap();
    std::fs::write(root.join("blob.rs"), b"fn binary() {}\0").unwrap();
    git(root, &["add", "src/lib.rs", ".gitignore"]);
    let cancelled = AtomicBool::new(false);
    let found = checkout_root(root.join("src").to_str().unwrap(), &cancelled).unwrap();
    assert_eq!(found.canonicalize().unwrap(), root.canonicalize().unwrap());
    let index = Index::build(root, &cancelled).unwrap();
    assert_eq!(
        index.files(),
        [".gitignore", "blob.rs", "src/lib.rs", "src/new.py"]
    );
    let symbols: Vec<_> = index
        .symbols()
        .iter()
        .map(|symbol| (symbol.name.as_str(), index.files()[symbol.file].as_str()))
        .collect();
    assert_eq!(
        symbols,
        [("tracked", "src/lib.rs"), ("untracked", "src/new.py")]
    );

    cancelled.store(true, Ordering::Release);
    assert!(Index::build(root, &cancelled).is_err());
}

#[cfg(unix)]
#[test]
fn symlinks_and_special_files_are_never_read() {
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("real.rs");
    std::fs::write(&target, "fn real() {}\n").unwrap();
    let link = dir.path().join("link.rs");
    std::os::unix::fs::symlink(&target, &link).unwrap();
    assert!(read_source(&target).is_some());
    assert!(read_source(&link).is_none());
    assert!(read_source(Path::new("/dev/null")).is_none());
}

#[cfg(unix)]
#[test]
fn a_tracked_folder_replaced_by_a_symlink_is_not_scanned() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("checkout");
    let outside = dir.path().join("outside");
    std::fs::create_dir_all(root.join("src/inner")).unwrap();
    std::fs::create_dir_all(&outside).unwrap();
    git(&root, &["init", "-q"]);
    std::fs::write(root.join("src/inner/lib.rs"), "fn inside() {}\n").unwrap();
    std::fs::write(root.join("main.rs"), "fn main() {}\n").unwrap();
    git(&root, &["add", "."]);
    std::fs::write(outside.join("lib.rs"), "fn outside() {}\n").unwrap();
    std::fs::remove_dir_all(root.join("src/inner")).unwrap();
    std::os::unix::fs::symlink(&outside, root.join("src/inner")).unwrap();
    let index = Index::build(&root, &AtomicBool::new(false)).unwrap();
    assert!(index.files().iter().any(|file| file == "src/inner/lib.rs"));
    let names: Vec<_> = index
        .symbols()
        .iter()
        .map(|symbol| symbol.name.as_str())
        .collect();
    assert_eq!(names, ["main"]);
}

#[test]
fn numstat_reads_counts_binaries_and_renames() {
    let output = b"3\t1\tsrc/a.rs\0-\t-\tlogo.png\0" as &[u8];
    let rename = b"2\t0\t\0old.rs\0src/new.rs\0" as &[u8];
    let counts = changes::numstat(&[output, rename].concat());
    assert_eq!(counts["src/a.rs"], Some((3, 1)));
    assert_eq!(counts["logo.png"], None);
    assert_eq!(counts["src/new.rs"], Some((2, 0)));
    assert!(!counts.contains_key("old.rs"));
    // A truncated rename is dropped, not misread.
    assert!(changes::numstat(b"1\t1\t\0old.rs").is_empty());
}

#[test]
fn uncommitted_changes_are_listed_newest_first() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    git(root, &["init", "-q"]);
    // As on Windows, where Git warns about line endings on every diff.
    git(root, &["config", "core.autocrlf", "true"]);
    for name in ["a.rs", "b.rs", "gone.rs", "same.rs"] {
        std::fs::write(root.join(name), "fn x() {}\n").unwrap();
    }
    git(root, &["add", "."]);
    git(root, &["commit", "-qm", "init"]);
    std::fs::write(root.join("a.rs"), "fn x() {}\nfn y() {}\n").unwrap();
    std::fs::write(root.join("b.rs"), "").unwrap();
    std::fs::write(root.join("new.rs"), "fn z() {}\n").unwrap();
    std::fs::remove_file(root.join("gone.rs")).unwrap();
    let at = |name: &str, seconds: u64| {
        let time = std::time::SystemTime::UNIX_EPOCH + Duration::from_secs(seconds);
        std::fs::File::options()
            .write(true)
            .open(root.join(name))
            .unwrap()
            .set_modified(time)
            .unwrap();
    };
    at("a.rs", 3_000);
    at("b.rs", 1_000);
    at("new.rs", 2_000);
    let index = Index::build(root, &AtomicBool::new(false)).unwrap();
    let changes: Vec<_> = index
        .changes()
        .iter()
        .map(|change| {
            (
                index.files()[change.file].as_str(),
                change.counts,
                change.new,
            )
        })
        .collect();
    assert_eq!(
        changes,
        [
            ("a.rs", Some((1, 0)), false),
            ("new.rs", None, true),
            ("b.rs", Some((0, 1)), false),
        ]
    );
    let same = index
        .files()
        .iter()
        .position(|name| name == "same.rs")
        .unwrap();
    assert!(index.change(same).is_none());
}

#[test]
fn a_checkout_without_commits_lists_its_files_as_new() {
    let dir = tempfile::tempdir().unwrap();
    git(dir.path(), &["init", "-q"]);
    std::fs::write(dir.path().join("a.rs"), "fn a() {}\n").unwrap();
    let index = Index::build(dir.path(), &AtomicBool::new(false)).unwrap();
    assert_eq!(index.changes().len(), 1);
    assert!(index.changes()[0].new);
}
