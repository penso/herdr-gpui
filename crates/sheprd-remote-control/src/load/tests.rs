use super::*;

#[test]
fn rounds_to_whole_percentages() {
    assert_eq!(
        Load::new(47.4, 2, 3, 8, Some(2.34)),
        Some(Load {
            cores: Some(8),
            cpu: 47,
            load5_tenths: Some(23),
            memory: 67
        })
    );
}

#[test]
fn clamps_out_of_range_and_nan_readings() {
    assert_eq!(
        Load::new(f32::NAN, 5, 4, 0, Some(f64::NAN)),
        Some(Load {
            cores: None,
            cpu: 0,
            load5_tenths: Some(0),
            memory: 100
        })
    );
    assert_eq!(
        Load::new(130., 0, 4, 1, None).map(|load| load.cpu),
        Some(100)
    );
}

#[test]
fn has_no_load_without_a_memory_total() {
    assert_eq!(Load::new(10., 0, 0, 8, None), None);
}

#[test]
fn a_load_average_the_os_does_not_keep_stays_off_the_wire() {
    let load = Load::new(10., 1, 2, 0, None);
    assert_eq!(
        serde_json::to_string(&load).ok().as_deref(),
        Some(r#"{"cpu":10,"memory":50}"#)
    );
}

#[test]
fn reads_a_load_from_an_older_daemon() {
    let load: Option<Load> = serde_json::from_str(r#"{"cpu":10,"memory":50}"#).ok();
    assert_eq!(load, Load::new(10., 1, 2, 0, None));
}
