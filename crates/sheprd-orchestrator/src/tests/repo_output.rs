use super::*;
use crate::repo::parse;
use sheprd_client::ConnectTarget;

#[test]
fn only_an_explicit_negative_probe_can_discard_a_project() {
    assert!(matches!(
        parse(&ConnectTarget::Local, "not-repository\n"),
        Err(Error::NotARepository)
    ));
    for output in [
        "",
        "unrecognized output\n",
        "common\t/r/.git\n",
        "main\t/r\n",
        "common\t\nmain\t/r\n",
        "common\t/r/.git\nmain\t\n",
        "not-repository\ncommon\t/r/.git\n",
    ] {
        assert!(
            matches!(
                parse(&ConnectTarget::Local, output),
                Err(Error::Output { .. })
            ),
            "{output:?}"
        );
    }
}
