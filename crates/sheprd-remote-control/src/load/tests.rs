use super::*;

#[test]
fn rounds_to_whole_percentages() {
    assert_eq!(
        Load::new(47.4, 2, 3),
        Some(Load {
            cpu: 47,
            memory: 67
        })
    );
}

#[test]
fn clamps_out_of_range_and_nan_readings() {
    assert_eq!(
        Load::new(f32::NAN, 5, 4),
        Some(Load {
            cpu: 0,
            memory: 100
        })
    );
    assert_eq!(Load::new(130., 0, 4).map(|load| load.cpu), Some(100));
}

#[test]
fn has_no_load_without_a_memory_total() {
    assert_eq!(Load::new(10., 0, 0), None);
}
