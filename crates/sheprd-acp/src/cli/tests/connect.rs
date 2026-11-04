use super::*;

#[test]
fn connect_takes_one_endpoint_id() {
    let id = "0123456789abcdef0123456789abcdef";
    assert_eq!(
        parse(args(&["connect", id])),
        Ok(Parsed::Connect(id.parse().unwrap()))
    );
    assert_eq!(parse(args(&["connect"])), Err(Error::MissingEndpoint));
    assert_eq!(
        parse(args(&["connect", "../etc"])),
        Err(Error::InvalidEndpoint("../etc".into()))
    );
    assert!(matches!(
        parse(args(&["connect", id, "extra"])),
        Err(Error::UnknownOption(_))
    ));
}

#[test]
fn agents_takes_nothing_more() {
    assert_eq!(parse(args(&["agents"])), Ok(Parsed::Agents));
    assert!(matches!(
        parse(args(&["agents", "x"])),
        Err(Error::UnknownOption(_))
    ));
}
