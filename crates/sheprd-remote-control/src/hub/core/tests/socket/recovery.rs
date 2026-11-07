use super::*;

#[test]
fn closed_stream_reattaches_at_the_same_address_and_types_on_the_new_channel() {
    let (source, fake) = fake_with_close(true);
    source.poll().unwrap();
    let params = json!({"pane_id": id("t1")});
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        let screen = source.call(ApiMethod::PaneRead, &params).unwrap();
        if screen["read"]["text"]
            .as_str()
            .unwrap()
            .contains("recovered")
        {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "closed attachment never reattached"
        );
        thread::yield_now();
    }
    source
        .call(
            ApiMethod::PaneSendText,
            &json!({"pane_id": id("t1"), "text": "new input"}),
        )
        .unwrap();
    assert_eq!(
        fake.inputs.recv_timeout(Duration::from_secs(2)).unwrap(),
        b"new input"
    );
    drop(source);
    fake.server.join().unwrap();
}
