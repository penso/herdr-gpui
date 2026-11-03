//! The fixture executable `scripts/test-updater.py` installs over and
//! restarts: the real update helper, plus the commands the script needs to
//! sign fixtures and observe a restart. Built with the deliberately public
//! `[42; 32]` test key and a fixed release version, never a production key.

use std::process::ExitCode;

fn main() -> anyhow::Result<ExitCode> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if let Some(code) = sheprd_updater::run_helper(&args) {
        return Ok(code);
    }
    match args.first().and_then(|arg| arg.to_str()) {
        // The script compares these with the values it built the fixture for.
        Some("--harness-identity") if args.len() == 1 => {
            let public: String = ed25519_dalek::SigningKey::from_bytes(&[42; 32])
                .verifying_key()
                .as_bytes()
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect();
            let embedded = option_env!("HERDR_UPDATE_PUBLIC_KEY");
            println!(
                "{}",
                serde_json::json!({
                    "version": sheprd_updater::APP_VERSION,
                    "key": embedded,
                    "fixture_key": public,
                })
            );
        }
        Some("--harness-sign") if args.len() == 1 => {
            use anyhow::Context as _;
            use ed25519_dalek::Signer as _;
            use std::io::Read as _;
            let mut bytes = Vec::new();
            std::io::stdin()
                .read_to_end(&mut bytes)
                .context("read signing input")?;
            let key = ed25519_dalek::SigningKey::from_bytes(&[42; 32]);
            println!(
                "{}",
                serde_json::to_string(&key.sign(&bytes).to_bytes().to_vec())?
            );
        }
        #[cfg(unix)]
        Some("--harness-restarted") if args.len() == 3 => {
            use anyhow::Context as _;
            use std::os::unix::ffi::OsStrExt as _;
            let report = serde_json::json!({
                "argument": args[2].as_bytes(),
                "cwd": std::env::current_dir()
                    .context("read restart working directory")?
                    .as_os_str()
                    .as_bytes(),
            });
            std::fs::write(&args[1], serde_json::to_vec(&report)?)
                .context("write restart report")?;
        }
        _ => return Ok(ExitCode::FAILURE),
    }
    Ok(ExitCode::SUCCESS)
}
