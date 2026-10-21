use crate::listening_ports::{STEP_TIMEOUT, local_shell, scan::COMMAND};

/// The step runs again and again in one long-lived shell, so it adds each
/// missing system directory to `PATH` once and leaves it alone afterwards
/// rather than growing it every scan.
#[test]
fn repeated_scans_add_missing_directories_once() {
    let cases = [
        ("/usr/bin:/bin", "/usr/bin:/bin:/usr/sbin:/sbin"),
        ("/usr/bin:/bin:/usr/sbin", "/usr/bin:/bin:/usr/sbin:/sbin"),
        ("/usr/bin:/bin:/sbin", "/usr/bin:/bin:/sbin:/usr/sbin"),
        (
            "/sbin:/usr/bin:/usr/sbin:/bin",
            "/sbin:/usr/bin:/usr/sbin:/bin",
        ),
    ];
    for (start, expected) in cases {
        let mut shell = local_shell().unwrap();
        shell.run(&format!("PATH='{start}'"), STEP_TIMEOUT).unwrap();
        for scan in 1..=3 {
            shell.run(COMMAND, STEP_TIMEOUT).unwrap();
            let path = shell
                .run("printf '%s\\n' \"$PATH\"", STEP_TIMEOUT)
                .unwrap()
                .stdout;
            assert_eq!(path.trim_end(), expected, "scan {scan} from {start:?}");
        }
    }
}
