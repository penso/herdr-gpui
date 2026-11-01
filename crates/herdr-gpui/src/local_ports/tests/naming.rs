use crate::local_ports::naming::{app_bundle, command_name, name, plist_string, project, system};
use std::path::Path;

fn words(line: &str) -> Vec<String> {
    line.split(' ').map(str::to_owned).collect()
}

#[test]
fn node_tools_are_named_by_the_package_they_run() {
    for (line, expected) in [
        ("node /home/dev/app/node_modules/.bin/vite", "vite"),
        (
            "node /home/dev/app/node_modules/vite/bin/vite.js --port 5173",
            "vite",
        ),
        (
            "node /home/dev/app/node_modules/next/dist/bin/next dev",
            "next dev",
        ),
        (
            "/usr/local/bin/node /usr/local/lib/node_modules/npm/bin/npm-cli.js run dev",
            "npm run dev",
        ),
        ("node --require ./trace.js server.js", "server.js"),
        (
            "node /home/dev/app/node_modules/@scope/tool/bin/cli.js serve",
            "tool serve",
        ),
        ("bun run dev", "dev"),
    ] {
        assert_eq!(
            command_name(&words(line)).as_deref(),
            Some(expected),
            "{line}"
        );
    }
}

#[test]
fn python_ruby_php_and_java_name_their_script() {
    for (line, expected) in [
        (
            "python3 manage.py runserver 0.0.0.0:8000",
            "manage.py runserver 0.0.0.0:8000",
        ),
        (
            "/usr/bin/python3.12 -m http.server 8000",
            "http.server 8000",
        ),
        ("python -u /srv/api/app.py", "app.py"),
        ("ruby bin/rails server", "rails server"),
        ("php artisan serve", "artisan serve"),
        (
            "java -Xmx512m -jar /opt/app/build/service.jar",
            "service.jar",
        ),
    ] {
        assert_eq!(
            command_name(&words(line)).as_deref(),
            Some(expected),
            "{line}"
        );
    }
}

#[test]
fn a_plain_program_or_a_bare_interpreter_keeps_its_own_name() {
    assert_eq!(command_name(&words("postgres -D /var/lib/data")), None);
    assert_eq!(command_name(&words("node")), None);
    assert_eq!(command_name(&[]), None);
    assert_eq!(
        name("postgres", None, &words("postgres -D /data")),
        "postgres"
    );
    assert_eq!(
        name("node", None, &words("node /srv/app/server.js")),
        "server.js"
    );
}

#[test]
fn a_derived_name_is_bounded_and_printable() {
    let long = format!("node /srv/{}.js", "x".repeat(200));
    let derived = command_name(&words(&long)).unwrap();
    assert!(derived.chars().count() <= 48, "{derived}");
    let control = vec!["node".to_owned(), "srv\u{1b}[31m.js".to_owned()];
    assert_eq!(command_name(&control).as_deref(), Some("srv[31m.js"));
}

#[test]
fn an_app_is_named_by_its_outermost_bundle() {
    let helper = Path::new(
        "/Applications/Example.app/Contents/Frameworks/Example Helper.app/Contents/MacOS/Example Helper",
    );
    assert_eq!(
        app_bundle(helper),
        Some(Path::new("/Applications/Example.app"))
    );
    assert_eq!(app_bundle(Path::new("/usr/local/bin/node")), None);
    // No Info.plist on disk: the bundle's folder names it.
    assert_eq!(
        name("Example Helper", Some(helper), &words("x --type=renderer")),
        "Example"
    );
}

#[test]
fn a_bundle_name_comes_from_an_xml_property_list() {
    let plist = "<plist><dict>\n\t<key>CFBundleName</key>\n\t<string>Example &amp; Co</string>\n\t<key>CFBundleDisplayName</key>\n\t<string>Example</string></dict></plist>";
    assert_eq!(
        plist_string(plist, "CFBundleDisplayName").as_deref(),
        Some("Example")
    );
    assert_eq!(
        plist_string(plist, "CFBundleName").as_deref(),
        Some("Example & Co")
    );
    assert_eq!(plist_string(plist, "CFBundleIdentifier"), None);
    assert_eq!(plist_string("bplist00\u{0}", "CFBundleName"), None);
}

#[test]
fn a_project_is_its_repository_or_its_folder() {
    let root = tempfile::tempdir().unwrap();
    let repository = root.path().join("shop");
    let nested = repository.join("apps/web");
    std::fs::create_dir_all(&nested).unwrap();
    std::fs::create_dir_all(repository.join(".git")).unwrap();
    assert_eq!(project(&nested).as_deref(), Some("shop"));
    let loose = root.path().join("scratch");
    std::fs::create_dir_all(&loose).unwrap();
    assert_eq!(project(&loose).as_deref(), Some("scratch"));
}

#[test]
fn well_known_system_listeners_are_told_apart() {
    for process in [
        "ControlCenter",
        "rapportd",
        "ARDAgent",
        "com.docker.backend",
    ] {
        assert!(system(process), "{process}");
    }
    for process in ["node", "postgres", "python3"] {
        assert!(!system(process), "{process}");
    }
}
