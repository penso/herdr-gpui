#![allow(clippy::unwrap_used)]

use super::{cpu_bar_fill, cpu_bar_text};
use crate::{
    config::Theme,
    system_load::{Monitor, Reading, Sample, SystemLoad},
    usage::Host,
};

impl SystemLoad {
    pub(crate) fn seed(&mut self, host: Host, sample: Sample) {
        let mut reading = Reading::default();
        reading.apply(Ok(sample));
        self.monitors.insert(
            host,
            Monitor {
                worker: None,
                reading,
            },
        );
    }
}

#[test]
fn cpu_bar_fill_tracks_the_share_and_severity_boundaries() {
    let theme = Theme::default();
    for (cpu, expected_color) in [
        (0., theme.ink(theme.palette[2])),
        (50., theme.ink(theme.palette[2])),
        (74.9, theme.ink(theme.palette[2])),
        (75., theme.ink(0xf59e0b)),
        (89.9, theme.ink(0xf59e0b)),
        (90., theme.ink(theme.palette[1])),
        (100., theme.ink(theme.palette[1])),
    ] {
        let mut reading = Reading::default();
        reading.apply(Ok(Sample {
            cpu: Some(cpu),
            ..Sample::default()
        }));
        for width in [0., 32., 180.] {
            let (fill, color) = cpu_bar_fill(&reading, &theme, width).unwrap();
            assert!(
                (fill - width * cpu / 100.).abs() < 0.001,
                "{cpu}% at {width}px"
            );
            assert_eq!(color, expected_color, "{cpu}%");
        }
    }
}

#[test]
fn cpu_bar_unknown_has_no_fill_and_stale_keeps_its_share() {
    let theme = Theme::default();
    let mut reading = Reading::default();
    assert_eq!(cpu_bar_fill(&reading, &theme, 120.), None);
    assert_eq!(cpu_bar_text(&reading), "CPU measuring");
    reading.apply(Ok(Sample::default()));
    assert_eq!(cpu_bar_fill(&reading, &theme, 120.), None);
    reading.apply(Err(crate::Error::SystemLoad(
        crate::system_load::Error::Output,
    )));
    assert_eq!(cpu_bar_fill(&reading, &theme, 120.), None);
    assert_eq!(cpu_bar_text(&reading), "CPU unavailable");

    reading.apply(Ok(Sample {
        cpu: Some(90.),
        ..Sample::default()
    }));
    assert_eq!(cpu_bar_text(&reading), "CPU 90%");
    reading.apply(Err(crate::Error::SystemLoad(
        crate::system_load::Error::Output,
    )));
    assert_eq!(
        cpu_bar_fill(&reading, &theme, 120.),
        Some((108., theme.muted))
    );
    assert_eq!(cpu_bar_text(&reading), "CPU 90%, stale");
}

#[test]
fn cpu_bar_rejects_nonfinite_shares_and_clamps_out_of_range_values() {
    let theme = Theme::default();
    for value in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        let mut reading = Reading::default();
        reading.apply(Ok(Sample {
            cpu: Some(value),
            ..Sample::default()
        }));
        assert_eq!(cpu_bar_fill(&reading, &theme, 120.), None);
        assert_eq!(cpu_bar_text(&reading), "CPU measuring");
    }
    for (value, expected_width, expected_text) in [(-1., 0., "CPU 0%"), (150., 120., "CPU 100%")] {
        let mut reading = Reading::default();
        reading.apply(Ok(Sample {
            cpu: Some(value),
            ..Sample::default()
        }));
        assert_eq!(
            cpu_bar_fill(&reading, &theme, 120.).unwrap().0,
            expected_width
        );
        assert_eq!(cpu_bar_text(&reading), expected_text);
    }
}
