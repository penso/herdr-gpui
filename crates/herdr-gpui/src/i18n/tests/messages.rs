use super::*;
use crate::controls::COMMANDS;

/// Catalog commands name their messages as data, which `fl!` cannot check.
#[test]
fn every_catalog_command_names_an_english_message_with_its_arguments() {
    for info in COMMANDS {
        let wanted: &[&str] = if info.command.number().is_some() {
            &["number"]
        } else {
            &[]
        };
        assert_eq!(takes(info.message), Some(wanted), "{}", info.message);
        let english = command_english(info);
        assert!(!english.is_empty() && !english.contains('{'), "{english}");
    }
}

#[test]
fn numbered_commands_say_their_number_in_english_without_isolation() {
    let info = COMMANDS
        .iter()
        .find(|info| info.command == crate::Command::TabNumber(3))
        .unwrap();
    assert_eq!(command_english(info), "Focus Tab 3");
    // The window's text isolates the number, as every placeable.
    assert_eq!(command(info), "Focus Tab \u{2068}3\u{2069}");
}

/// The build script refuses files that do not parse; what it embeds is
/// every English file, and the table `fl!` searches is sorted.
#[test]
fn the_embedded_files_are_the_english_ones_and_the_table_is_sorted() {
    assert!(
        FILES
            .iter()
            .any(|file| file.language == "en-US" && file.feature == "commands")
    );
    assert!(FILES.iter().all(|file| file.language == "en-US"));
    assert!(MESSAGES.windows(2).all(|pair| pair[0].0 < pair[1].0));
    assert_eq!(available(), vec![Locale::FALLBACK]);
}
