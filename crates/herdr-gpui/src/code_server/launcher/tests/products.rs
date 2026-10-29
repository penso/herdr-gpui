//! VSCodium serves VS Code tabs as VS Code does, without a license to accept.
use super::*;
use crate::code_server::VSCODIUM;

fn codium() -> Cli {
    Cli::Found(Install::new(
        &VSCODIUM,
        "/Applications/VSCodium.app/Contents/Resources/app/bin/codium-tunnel",
    ))
}

#[test]
fn vscodium_starts_without_a_license() {
    let token = PathBuf::from("/state/vscode-token");
    let blank = CodeConfig::default();
    assert_eq!(mode(&blank, &codium()), Some(CodeMode::Start));
    assert!(matches!(
        startup(true, &blank, &codium(), false, None),
        Startup::Idle
    ));
    let plan = plan(true, &blank, &codium(), token).unwrap();
    assert_eq!(
        plan.program,
        PathBuf::from("/Applications/VSCodium.app/Contents/Resources/app/bin/codium-tunnel")
    );
    // VS Code still waits for its license.
    assert!(matches!(
        startup(true, &blank, &found(), false, None),
        Startup::Consent
    ));
}

#[cfg(any(target_os = "macos", windows))]
#[gpui::test]
fn vscodium_keeps_running_without_a_license(cx: &mut gpui::TestAppContext) {
    cx.update(|cx| {
        Launcher::fixture(cx, codium());
        assert_eq!(
            Launcher::product(cx).map(Product::name),
            Some(VSCODIUM.name())
        );
        let code = CodeConfig::default();
        Launcher::want(cx, &code);
        Launcher::sync(cx, &code);
        assert!(Launcher::running(cx));
    });
}
