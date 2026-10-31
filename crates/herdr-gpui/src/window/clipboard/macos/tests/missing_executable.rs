use super::*;

#[test]
fn a_removed_app_file_says_to_restart_rather_than_failing_to_spawn() {
    let gone = std::env::temp_dir().join("herdr-removed-app/Contents/MacOS/Herdr");
    let error = reader_command(&gone, false).err();
    assert!(matches!(&error, Some(Error::Missing(path)) if path == &gone));
    assert!(
        error
            .map(|error| error.to_string())
            .is_some_and(|text| text.contains("Restart Herdr"))
    );
}

#[test]
fn an_existing_executable_is_run_as_the_reader() -> Result<(), Error> {
    let command = reader_command(Path::new("/bin/sh"), true)?;
    let args: Vec<_> = command.get_args().collect();
    assert_eq!(args, [HELPER, "image"]);
    Ok(())
}
