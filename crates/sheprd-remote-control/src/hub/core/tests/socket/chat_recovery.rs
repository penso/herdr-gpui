use super::*;
use sheprd_chat_protocol::{Agent, ENDPOINT_ENV, TOKEN};
use std::path::Path;

#[test]
fn a_chat_cannot_recover_as_a_shell_with_a_stale_endpoint_label() {
    let (source, fake) = fake();
    source.poll().unwrap();
    source
        .open_chat(Agent::Claude, Path::new("/bin/sheprd-acp"), "/code")
        .unwrap();
    let request = fake.inputs.recv_timeout(Duration::from_secs(2)).unwrap();
    let request: Value = serde_json::from_slice(&request).unwrap();
    assert_eq!(request["method"], "terminal.run");
    let params = &request["params"];
    assert_eq!(params["keep"], false);
    assert!(params["labels"][TOKEN].as_str().is_some());
    assert_eq!(params["labels"][TOKEN], params["env"][ENDPOINT_ENV]);

    // Ordinary shells still recover at the same address after program loss.
    source.open_shell("/code").unwrap();
    let request = fake.inputs.recv_timeout(Duration::from_secs(2)).unwrap();
    let request: Value = serde_json::from_slice(&request).unwrap();
    assert_eq!(request["params"]["keep"], true);
    drop(source);
    fake.server.join().unwrap();
}
