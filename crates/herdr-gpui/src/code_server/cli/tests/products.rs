use super::*;

/// Installs `product`'s command and the program beside it that serves in
/// `bin`, as its app bundle has them.
fn install(bin: &Path, product: &Product) {
    program(&bin.join(product.command));
    program(&bin.join(product.server));
}

fn found(path: Option<&OsStr>, bundles: [&[&Path]; 2]) -> Option<Install> {
    find_in(path, [(&VS_CODE, bundles[0]), (&VSCODIUM, bundles[1])])
}

#[test]
fn vscodium_alone_serves_with_its_own_tunnel() {
    let temp = tempfile::tempdir().unwrap();
    let bin = temp.path().join("VSCodium").join("bin");
    install(&bin, &VSCODIUM);
    let path = joined(&[&bin]);
    let canonical = std::fs::canonicalize(&bin).unwrap();
    assert_eq!(
        found(Some(&path), [&[], &[]]),
        Some(Install::new(&VSCODIUM, canonical.join(VSCODIUM.server)))
    );
    // Its bundle is found without PATH too.
    let bundled = bin.join(VSCODIUM.command);
    assert_eq!(
        found(None, [&[], &[bundled.as_path()]]),
        Some(Install::new(&VSCODIUM, canonical.join(VSCODIUM.server)))
    );
}

#[test]
fn vs_code_wins_wherever_each_is_found() {
    let temp = tempfile::tempdir().unwrap();
    let (code, codium) = (temp.path().join("code"), temp.path().join("codium"));
    install(&code, &VS_CODE);
    install(&codium, &VSCODIUM);
    let vs_code = Install::new(
        &VS_CODE,
        std::fs::canonicalize(&code).unwrap().join(VS_CODE.server),
    );
    // VSCodium earlier on PATH still loses.
    let path = joined(&[&codium, &code]);
    assert_eq!(found(Some(&path), [&[], &[]]), Some(vs_code.clone()));
    // So it does to a VS Code that is only in its bundle.
    let path = joined(&[&codium]);
    let bundled = code.join(VS_CODE.command);
    assert_eq!(
        found(Some(&path), [&[bundled.as_path()], &[]]),
        Some(vs_code)
    );
}

#[test]
fn neither_installed_finds_nothing() {
    let temp = tempfile::tempdir().unwrap();
    let empty = temp.path().join("empty");
    std::fs::create_dir_all(&empty).unwrap();
    let missing = temp.path().join("missing").join(VSCODIUM.command);
    let path = joined(&[&empty]);
    assert_eq!(found(Some(&path), [&[], &[missing.as_path()]]), None);
}

#[test]
fn only_vs_code_asks_for_a_license() {
    assert!(VS_CODE.licensed());
    assert!(!VSCODIUM.licensed());
    let names = product_names();
    assert!(
        PRODUCTS
            .iter()
            .all(|product| names.contains(product.name()))
    );
}
