use super::*;
use crate::script::shell_quote;

const HOSTILE_PATHS: [&str; 6] = [
    "/srv/a b",
    "/srv/it's",
    "/srv/-dash",
    "/srv/$HOME",
    "/srv/`id`",
    "/srv/\u{e9}\u{2019}",
];

#[test]
fn posix_scripts_assign_the_path_as_one_quoted_word() {
    for path in HOSTILE_PATHS {
        let assignment = format!("path={}\n", shell_quote(path));
        assert!(
            posix::fetch_script(path, 10).starts_with(&assignment),
            "{path}"
        );
        for expect in [Expect::Any, Expect::Absent, Expect::Sha256([0; 32])] {
            let script = posix::push_script(path, expect, Access::Keep, 0);
            assert!(script.contains(&format!("\n{assignment}")), "{path}");
        }
    }
    assert!(posix::fetch_script("/srv/it's", 10).starts_with("path='/srv/it'\\''s'\n"));
}

#[test]
fn posix_scripts_expand_only_a_leading_home() {
    let fetch = |path| posix::fetch_script(path, 10);
    assert!(fetch("~/a b").starts_with("path=\"$HOME\"/'a b'\n"));
    assert!(fetch("~/-x").starts_with("path=\"$HOME\"/'-x'\n"));
    assert!(fetch("~").starts_with("path=\"$HOME\"\n"));
    assert!(fetch("~root/x").starts_with("path='~root/x'\n"));
    assert!(fetch("~\\x").starts_with("path='~\\x'\n"));
    assert!(fetch("/srv/~/x").starts_with("path='/srv/~/x'\n"));
}

#[test]
fn posix_scripts_never_pass_the_path_where_it_could_be_an_option() {
    let fetch = posix::fetch_script("-x", 10);
    assert!(fetch.contains("wc -c < \"$path\""));
    assert!(fetch.contains("exec cat < \"$path\""));
    let push = posix::push_script("-x", Expect::Sha256([0; 32]), Access::Keep, 0);
    assert!(push.contains("$hasher < \"$target\""));
    assert!(push.contains("mv -f $no_follow -- \"$copy\" \"$target\""));
    assert!(push.contains("\ntarget=../$name\n"));
}

#[test]
fn posix_push_consults_a_hasher_only_for_an_expected_hash() {
    let digest = [0xAB; 32];
    let script = posix::push_script("/f", Expect::Sha256(digest), Access::Keep, 0);
    assert!(script.contains("sha256sum"));
    assert!(script.contains(&format!("!= '{}'", "ab".repeat(32))));
    for expect in [Expect::Absent, Expect::Any] {
        assert!(!posix::push_script("/f", expect, Access::Keep, 0).contains("sha256sum"));
    }
}

#[test]
fn pushes_check_the_received_length_before_the_expectation() {
    let posix = posix::push_script("/f", Expect::Sha256([0; 32]), Access::Keep, 42);
    let counted = posix.find("-ne 42 ]; then exit 70").unwrap();
    assert!(counted < posix.find("$hasher < \"$target\"").unwrap());
    assert!(counted < posix.find("mv -f").unwrap());
    assert!(
        posix::push_script("/f", Expect::Any, Access::Keep, u64::MAX)
            .contains(&format!("-ne {} ]", i64::MAX))
    );
    let powershell = powershell::push_script("C:\\f", Expect::Sha256([0; 32]), Access::Keep, 42);
    let counted = powershell.find("($received -ne 42) { exit 70 }").unwrap();
    assert!(counted < powershell.find("ComputeHash").unwrap());
    assert!(counted < powershell.find("::Replace(").unwrap());
}

#[test]
fn posix_push_names_where_a_new_files_access_comes_from() {
    let keep = posix::push_script("/f", Expect::Absent, Access::Keep, 0);
    assert!(keep.contains("\nsource=\n"));
    let like = posix::push_script("/f.conflict", Expect::Absent, Access::Like("~/a b"), 0);
    assert!(like.contains("\nsource=\"$HOME\"/'a b'\n"));
    let hostile = posix::push_script("/f", Expect::Absent, Access::Like("/srv/it's"), 0);
    assert!(hostile.contains("\nsource='/srv/it'\\''s'\n"));
}

#[test]
fn posix_push_checks_it_can_write_in_place_before_writing() {
    let script = posix::push_script("/f", Expect::Any, Access::Keep, 0);
    let shared = script
        .find("if [ -z \"$safe\" ]; then exit 72; fi")
        .unwrap();
    let checked = script
        .find("if [ ! -w \"$target\" ]; then exit 71; fi")
        .unwrap();
    let written = script.find("cat < \"$copy\" > \"$target\"").unwrap();
    assert!(shared < written && checked < written);
    // "Keep both" checks for a file that appeared before writing in place
    // and again just before the rename.
    let absent = posix::push_script("/f", Expect::Absent, Access::Keep, 0);
    let check = "if [ -e \"$target\" ] || [ -L \"$target\" ]; then exit 75; fi";
    let in_place = absent.find("\nelif [ -e \"$target\" ]; then\n").unwrap();
    let conflict = in_place + absent[in_place..].find(check).unwrap();
    assert!(conflict < absent.find("cat < \"$copy\" > \"$target\"").unwrap());
    let renamed = absent.find("\nmv -f $no_follow").unwrap();
    let last = absent.rfind(check).unwrap();
    assert!(absent.find("probe_to=$(mktemp").unwrap() < last && last < renamed);
    assert!(
        script.contains("??????????[+@]*"),
        "macOS marks an ACL with @ too"
    );
    assert!(
        script.contains("getfacl -s -- \"$listed\""),
        "BusyBox ls marks none"
    );
}

#[test]
fn powershell_push_reads_the_acl_before_creating_the_copy() {
    let like =
        powershell::push_script("C:\\f.conflict", Expect::Absent, Access::Like("C:\\a b"), 0);
    assert!(like.contains("\n$source = [System.IO.Path]::GetFullPath('C:\\a b')\n"));
    // A source that is not a file is refused, then one whose ACL cannot be
    // read, both before the copy exists.
    let not_a_file = like.find("Get-HerdrAttributes $like").unwrap();
    let read = like.find("GetAccessControl(").unwrap();
    let refused = like.rfind("exit 71").unwrap();
    let created = like.find("FileMode]::CreateNew").unwrap();
    assert!(not_a_file < read && read < refused && refused < created);
    // A source the user cannot stat fails inside the `try`, so it is refused
    // too rather than ending the script with a generic failure.
    let guarded = like.find("try {\n        $likeAttributes = Get-HerdrAttributes $like");
    assert!(guarded.is_some(), "{like}");
    assert!(like.contains("FileSystemAclExtensions]::Create("));
    let keep = powershell::push_script("C:\\f", Expect::Absent, Access::Keep, 0);
    assert!(keep.contains("\n$source = $null\n"));
}

#[test]
fn posix_fetch_limit_stays_in_signed_shell_arithmetic() {
    assert!(posix::fetch_script("/f", u64::MAX).contains(&format!("-gt {} ]", i64::MAX)));
    assert!(posix::fetch_script("/f", 42).contains("-gt 42 ]"));
}

#[test]
fn powershell_scripts_quote_the_path_as_a_literal() {
    let path = "C:\\a b\\it's \u{2018}x\u{2019} $env:X `y \u{e9}";
    let quoted = "'C:\\a b\\it''s \u{2018}\u{2018}x\u{2019}\u{2019} $env:X `y \u{e9}'";
    assert!(powershell::fetch_script(path, 10).contains(&format!("\n$path = {quoted}\n")));
    let push = powershell::push_script(path, Expect::Any, Access::Keep, 0);
    assert!(push.contains(&format!("GetFullPath({quoted})")));
    assert!(powershell::fetch_script("-x", 10).contains("\n$path = '-x'\n"));
    for path in HOSTILE_PATHS {
        let script = powershell::fetch_script(path, 10);
        let line = script.lines().nth(2).unwrap();
        assert!(
            line.starts_with("$path = '") && line.ends_with('\''),
            "{line}"
        );
    }
}

#[test]
fn powershell_scripts_expand_only_a_leading_home() {
    let combine = "[System.IO.Path]::Combine($env:USERPROFILE, 'a b')";
    assert!(powershell::fetch_script("~\\a b", 10).contains(combine));
    assert!(powershell::fetch_script("~/a b", 10).contains(combine));
    assert!(powershell::fetch_script("~", 10).contains("\n$path = $env:USERPROFILE\n"));
    assert!(powershell::fetch_script("~x\\a", 10).contains("\n$path = '~x\\a'\n"));
}

#[test]
fn powershell_push_checks_each_expectation() {
    let script = powershell::push_script("C:\\f", Expect::Sha256([0x0F; 32]), Access::Keep, 0);
    assert!(script.contains("SHA256]::Create().ComputeHash($current)"));
    assert!(script.contains(&format!("-ne '{}'", "0f".repeat(32))));
    let absent = powershell::push_script("C:\\f", Expect::Absent, Access::Keep, 0);
    assert!(absent.contains("Get-HerdrAttributes $path)) { exit 75 }"));
    assert!(!absent.contains("ComputeHash"));
    let any = powershell::push_script("C:\\f", Expect::Any, Access::Keep, 0);
    assert!(!any.contains("ComputeHash") && !any.contains("exit 75"));
    for script in [script, absent, any] {
        assert!(!script.contains("$input"));
        assert!(script.contains("[Console]::OpenStandardInput()"));
        assert!(
            script.contains("[System.IO.File]::Replace($temporary, $path, [NullString]::Value)")
        );
    }
}

#[test]
fn encoded_scripts_decode_back_to_the_script() {
    let script = powershell::push_script("C:\\\u{e9}\u{1F600}'", Expect::Absent, Access::Keep, 0);
    assert_eq!(decode(&encode(&script)), script);
}

/// The SSH command line carries exactly the encoding the local PowerShell
/// tests run, so they prove what a Windows host receives.
#[test]
fn powershell_ssh_command_carries_the_encoded_script() {
    let script = powershell::fetch_script("~\\it's", 10);
    let command = crate::ssh::powershell_script_command("user@host", &script).unwrap();
    let remote = command
        .get_args()
        .last()
        .unwrap()
        .to_str()
        .unwrap()
        .to_owned();
    let encoded = remote
        .strip_prefix("powershell.exe -NoLogo -NoProfile -NonInteractive -EncodedCommand ")
        .unwrap();
    assert_eq!(encoded, encode(&script));
    assert_eq!(decode(encoded), script);
}

#[test]
fn hex_is_lowercase_and_padded() {
    let mut digest = [0u8; 32];
    digest[0] = 0x0A;
    digest[31] = 0xF0;
    let text = hex(&digest);
    assert_eq!(text.len(), 64);
    assert!(text.starts_with("0a00") && text.ends_with("00f0"));
}

#[cfg(unix)]
fn exit(code: i32) -> crate::Error {
    use std::os::unix::process::ExitStatusExt;
    crate::Error::ScriptExit {
        status: std::process::ExitStatus::from_raw(code << 8),
        stderr: String::new(),
    }
}

#[cfg(windows)]
fn exit(code: i32) -> crate::Error {
    use std::os::windows::process::ExitStatusExt;
    crate::Error::ScriptExit {
        status: std::process::ExitStatus::from_raw(code.unsigned_abs()),
        stderr: String::new(),
    }
}

#[test]
fn exit_codes_map_to_refusals() {
    let limit = Some(MAX_REMOTE_FILE_BYTES);
    assert!(matches!(classify(exit(65), limit), Error::Symlink));
    assert!(matches!(classify(exit(66), limit), Error::Missing));
    assert!(matches!(classify(exit(67), None), Error::NotAFile));
    assert!(matches!(
        classify(exit(68), limit),
        Error::TooLarge {
            limit: MAX_REMOTE_FILE_BYTES
        }
    ));
    assert!(matches!(classify(exit(69), None), Error::NoHasher));
    assert!(matches!(classify(exit(70), None), Error::Incomplete));
    assert!(matches!(classify(exit(71), None), Error::Access));
    assert!(matches!(classify(exit(72), None), Error::SharedFolder));
    assert!(matches!(classify(exit(75), None), Error::Conflict));
    assert!(matches!(
        classify(crate::Error::ScriptOutputLimit, Some(7)),
        Error::TooLarge { limit: 7 }
    ));
}

#[test]
fn other_failures_keep_the_script_error() {
    let failed = classify(exit(1), Some(1));
    assert!(
        matches!(script_error(&failed), Some(crate::Error::ScriptExit { status, .. }) if status.code() == Some(1))
    );
    assert!(matches!(
        script_error(&classify(exit(68), None)),
        Some(crate::Error::ScriptExit { .. })
    ));
    assert!(matches!(
        script_error(&classify(crate::Error::ScriptOutputLimit, None)),
        Some(crate::Error::ScriptOutputLimit)
    ));
    let cancelled = classify(crate::Error::ScriptCancelled, Some(1));
    assert!(matches!(
        script_error(&cancelled),
        Some(crate::Error::ScriptCancelled)
    ));
    let source = std::error::Error::source(&cancelled).unwrap();
    assert_eq!(
        source.to_string(),
        crate::Error::ScriptCancelled.to_string()
    );
}

#[test]
fn messages_are_short_and_plain() {
    let too_large = Error::TooLarge {
        limit: MAX_REMOTE_FILE_BYTES,
    };
    assert_eq!(too_large.to_string(), "the file is larger than 64 MB");
    assert_eq!(
        Error::TooLarge { limit: 1 }.to_string(),
        "the file is larger than 1 MB"
    );
    let root = crate::Error::from(Error::Conflict);
    assert!(matches!(root, crate::Error::RemoteFile(Error::Conflict)));
    assert_eq!(
        root.to_string(),
        "the host's file changed since it was fetched"
    );
    assert_eq!(Error::Missing.to_string(), "the file is not on the host");
    assert_eq!(
        Error::Incomplete.to_string(),
        "the upload was cut off; the host's file is unchanged"
    );
    assert_eq!(
        Error::Access.to_string(),
        "could not keep the file's access; nothing was changed"
    );
    assert_eq!(
        Error::SharedFolder.to_string(),
        "other users can replace files in that folder on the host, so it was not written in \
         place; nothing was changed"
    );
}

#[test]
fn probe_answers_are_four_strict_lines() {
    let digest = "ab".repeat(32);
    let valid = format!("12\n{digest}\nsrc\n0\n");
    assert_eq!(
        parse_probe_output(valid.as_bytes()).unwrap(),
        ProbedFolder {
            bytes: 12 * 1024,
            fingerprint: Some([0xab; 32]),
            top: "src".into(),
            links_skipped: 0,
        }
    );
    let absent = parse_probe_output(b"12\n-\nsrc\n7\n").unwrap();
    assert_eq!(absent.fingerprint, None);
    assert_eq!(absent.links_skipped, 7);
    assert_eq!(parse_probe_output(b"12\n-\n.\n0\n").unwrap().top, ".");

    let bad = [
        "12\n-\nsrc\n".to_owned(),
        "12\n-\nsrc\n0\n5\n".to_owned(),
        "12\n-\n\n0\n".to_owned(),
        "12\n-\na/b\n0\n".to_owned(),
        "12\n-\na\u{7}b\n0\n".to_owned(),
        format!("12\n+{digest}\nsrc\n0\n"),
        format!("12\n{}\nsrc\n0\n", "A".repeat(64)),
        format!("12\n{}\nsrc\n0\n", "a".repeat(65)),
        "12\n-\nsrc\n+0\n".to_owned(),
        "12\n-\nsrc\n 0\n".to_owned(),
        format!("12\n-\nsrc\n{}\n", u64::from(u32::MAX) + 1),
    ];
    for answer in bad {
        assert!(
            matches!(
                parse_probe_output(answer.as_bytes()),
                Err(Error::FolderProbe)
            ),
            "{answer:?}"
        );
    }
}

fn file<'a>(target: &'a str, shell: RemoteShell, path: &'a str) -> RemoteFile<'a> {
    RemoteFile {
        target,
        shell,
        path,
    }
}

#[test]
fn public_functions_refuse_an_invalid_target() {
    let cancelled = AtomicBool::new(false);
    let posix = file("-oProxyCommand=x", RemoteShell::Posix, "/f");
    let fetched = fetch_remote_file(posix, Vec::new(), 1, &cancelled);
    assert!(matches!(fetched, Err(crate::Error::InvalidSshTarget)));
    let windows = file("-oProxyCommand=x", RemoteShell::PowerShell, "C:\\f");
    let pushed = push_remote_file(windows, Expect::Any, Access::Keep, 0, &[][..], &cancelled);
    assert!(matches!(pushed, Err(crate::Error::InvalidSshTarget)));
}

/// Every client, Windows included, runs a host's scripts through its own
/// `ssh`; `/bin/sh` and `powershell.exe` are the host's, not the client's.
#[test]
fn remote_command_runs_ssh_for_either_host_shell() {
    let cases = [
        (RemoteShell::Posix, "/bin/sh -c "),
        (RemoteShell::PowerShell, "powershell.exe "),
    ];
    for (shell, remote) in cases {
        let command = remote_command("user@host", shell, "exit 0").unwrap();
        let program = Path::new(command.get_program());
        assert_eq!(program.file_stem().unwrap(), "ssh", "{shell:?}");
        let args: Vec<_> = command
            .get_args()
            .map(|arg| arg.to_str().unwrap())
            .collect();
        assert!(args.contains(&"user@host"), "{shell:?}: {args:?}");
        assert!(
            args.last().unwrap().starts_with(remote),
            "{shell:?}: {args:?}"
        );
    }
}
