use super::*;
use crate::{
    i18n,
    system_load::{
        gpu::Gpu,
        render::details,
        sample::{Disk, Memory},
    },
};
use herdr_i18n::{Locale, Pseudo};

#[test]
fn every_tooltip_line_is_a_message_under_en_xa() {
    let memory = Memory {
        used: 6 << 30,
        total: 16 << 30,
    };
    let mut reading = Reading::default();
    reading.apply(Ok(Sample {
        cpu: Some(12.),
        memory: Some(memory),
        cores: Some(8),
        load: Some([1., 2., 3.]),
        disk: Some(Disk {
            available: 100 << 30,
            total: 500 << 30,
        }),
        uptime: Some(3 * 86_400),
        gpu: Some(Gpu {
            busy: Some(40.),
            memory: Some(memory),
            count: 2,
        }),
    }));
    i18n::show(&Pseudo::Accented.locale());
    let mut shown = details(&reading, &Host::Local);
    reading.apply(Err(Error::Remote(Box::new(
        herdr_usage::Error::Unreachable.into(),
    ))
    .into()));
    shown.push('\n');
    shown.push_str(&details(&reading, &Host::Wsl("Ubuntu".into())));
    i18n::show(&Locale::FALLBACK);
    for line in shown.lines() {
        assert!(herdr_i18n::pseudo::is_marked(line), "{line}");
    }
}
