use super::{
    DeviceSize, Price, SandboxSize, VmSpec, arch, device_size, price, sandbox_size, vm_spec,
};
use herdr_i18n::{Locale, Pseudo};

fn english(text: String) -> String {
    text.replace(['\u{2068}', '\u{2069}'], "")
}

const SPEC: VmSpec = VmSpec {
    cores: 2,
    memory_mb: 8192,
};

#[test]
fn sizes_and_prices_read_as_before_in_english() {
    crate::i18n::show(&Locale::FALLBACK);
    assert_eq!(
        english(vm_spec(SPEC, "Standard_D2pds_v6")),
        "2 vCPU · 8 GiB · Arm"
    );
    assert_eq!(
        english(vm_spec(
            VmSpec {
                cores: 1,
                memory_mb: 1536
            },
            "Standard_D1s_v5"
        )),
        "1 vCPU · 1.5 GiB · x64"
    );
    assert_eq!(
        english(sandbox_size(SandboxSize::standard(500))),
        "0.5 vCPU · 1 GiB"
    );
    assert_eq!(english(price(Price(0.00949), true)), "$0.0095/hr · ~$7/mo");
    assert_eq!(english(price(Price(1.5), false)), "$1.50/hr");
    assert_eq!(
        english(device_size(&DeviceSize::Vm {
            size: "Standard_D2pds_v6".into(),
            spec: Some(SPEC),
            spot: true,
        })),
        "Standard_D2pds_v6 · 2 vCPU · 8 GiB · Arm · Spot"
    );
    assert_eq!(
        english(device_size(&DeviceSize::Vm {
            size: "Standard_D2s_v5".into(),
            spec: None,
            spot: false,
        })),
        "Standard_D2s_v5 · regular price"
    );
    assert_eq!(
        english(device_size(&DeviceSize::Sandbox(SandboxSize::standard(
            2000
        )))),
        "2 vCPU · 4 GiB"
    );
}

#[test]
fn every_size_line_is_a_message_under_en_xa() {
    crate::i18n::show(&Pseudo::Accented.locale());
    let lines = [
        vm_spec(SPEC, "Standard_D2pds_v6"),
        arch("Standard_D2s_v5"),
        sandbox_size(SandboxSize::standard(1000)),
        price(Price(0.2), true),
        price(Price(0.2), false),
        device_size(&DeviceSize::Vm {
            size: "Standard_D2pds_v6".into(),
            spec: Some(SPEC),
            spot: false,
        }),
        device_size(&DeviceSize::Vm {
            size: "Standard_D2pds_v6".into(),
            spec: None,
            spot: true,
        }),
    ];
    crate::i18n::show(&Locale::FALLBACK);
    for line in lines {
        assert!(herdr_i18n::pseudo::is_marked(&line), "{line}");
    }
}
