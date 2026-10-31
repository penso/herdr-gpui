use super::*;

#[test]
fn batched_metadata_requires_exact_complete_ordered_frames() {
    let names = vec!["snapshot".into(), "w1:p1".into(), "w1:p2".into()];
    let valid = "\x1esnapshot\n{\"snapshot\":1}\n\x1ew1:p1\n{}\n\x1ew1:p2\n\x1eend\n";
    assert_eq!(
        batch::frames(valid.as_bytes(), &names).unwrap(),
        ["{\"snapshot\":1}\n", "{}\n", ""]
    );
    for invalid in [
        valid.trim_end_matches("\x1eend\n").to_owned(),
        format!("banner\n{valid}"),
        format!("{valid}extra"),
        format!("{valid}\x1eend\n"),
        valid.replace("\x1ew1:p2\n", "\x1ew1:p1\n"),
        valid.replace("\x1ew1:p1\n", "\x1ew9:p9\n"),
    ] {
        assert!(matches!(
            batch::frames(invalid.as_bytes(), &names),
            Err(Error::Framing)
        ));
    }
    assert!(matches!(
        batch::frames(&[0xff], &names),
        Err(Error::Framing)
    ));
}

#[test]
fn batches_keep_separate_snapshot_and_total_metadata_limits() {
    let names = vec!["snapshot".into(), "1".into(), "2".into()];
    let snapshot = "x".repeat(LIMITS.output as usize);
    let half = "x".repeat((LIMITS.output as usize - 6) / 2);
    let boundary = format!("\x1esnapshot\n{snapshot}\x1e1\n{half}\x1e2\n{half}\x1eend\n");
    assert!(batch::frames(boundary.as_bytes(), &names).is_ok());
    for too_large in [
        boundary.replace("\x1e1\n", "x\x1e1\n"),
        boundary.replace("\x1eend\n", "x\x1eend\n"),
    ] {
        assert!(matches!(
            batch::frames(too_large.as_bytes(), &names),
            Err(Error::Framing)
        ));
    }
}

#[cfg(target_os = "linux")]
#[test]
fn mixed_candidate_metadata_does_not_attach_an_unselected_pane() {
    let (_temp, host) = host::fixture(false, false);
    let result = inspect_host(
        &host,
        &scope(),
        &["w1:p1".into(), "w9:p9".into()],
        &AtomicBool::new(false),
    )
    .unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].pane_id, "w1:p1");
}
