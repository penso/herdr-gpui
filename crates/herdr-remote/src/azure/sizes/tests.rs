#![allow(clippy::unwrap_used)]
use super::*;

fn item(sku: &str, sku_name: &str, product: &str, price: f64) -> PriceItem {
    PriceItem {
        sku: sku.into(),
        sku_name: sku_name.into(),
        product: product.into(),
        meter: String::new(),
        price,
        currency: "USD".into(),
    }
}

#[test]
fn arm_sizes_are_told_by_their_feature_letters() {
    for arm in [
        "Standard_B2pts_v2",
        "Standard_D2ps_v6",
        "Standard_D4ps_v5",
        "Standard_E2pds_v5",
    ] {
        assert!(is_arm(arm), "{arm}");
    }
    for x64 in [
        "Standard_B2s_v2",
        "Standard_D4s_v5",
        "Standard_DS1_v2",
        "Standard_NP10s",
        "Standard_M416-208s_v2",
        "Standard_B2ms",
    ] {
        assert!(!is_arm(x64), "{x64}");
    }
}

#[test]
fn every_listed_size_is_a_plain_name() {
    for (index, size) in VM_CHOICES.iter().enumerate() {
        assert!(plain(size));
        assert!(!VM_CHOICES[..index].contains(size));
    }
    assert!(VM_CHOICES.contains(&super::super::settings::DEFAULT_VM_SIZE));
    assert!(
        VM_CHOICES.iter().all(|size| is_arm(size)),
        "x64 sizes are typed in"
    );
    assert!(!plain("Standard' or 1 eq 1"));
}

#[test]
fn only_the_linux_pay_as_you_go_price_is_kept() {
    let prices = linux_prices(vec![
        item(
            "Standard_B2pts_v2",
            "B2pts v2 Spot",
            "Virtual Machines Bpsv2 Series",
            0.0075,
        ),
        item(
            "Standard_B2pts_v2",
            "B2pts v2 Low Priority",
            "Virtual Machines Bpsv2 Series",
            0.0019,
        ),
        item(
            "Standard_B2pts_v2",
            "B2pts v2",
            "Virtual Machines Bpsv2 Series Windows",
            0.0187,
        ),
        item(
            "Standard_B2pts_v2",
            "B2pts v2",
            "Bpsv2 Series Cloud Services",
            0.0187,
        ),
        item(
            "Standard_B2pts_v2",
            "B2pts v2",
            "Virtual Machines Bpsv2 Series",
            0.00949,
        ),
    ]);
    assert_eq!(prices.len(), 1);
    assert_eq!(prices["Standard_B2pts_v2"], Price(0.00949));
    assert!((Price(0.00949).monthly() - 6.9277).abs() < 1e-9);
}

#[test]
fn sandbox_sizes_get_the_most_memory_the_service_allows() {
    let size = SandboxSize::parse("2").unwrap();
    assert_eq!(
        (size.cpu(), size.memory()),
        ("2000m".into(), "4096Mi".into())
    );
    assert_eq!((size.cores(), size.disk_gib()), ("2".into(), 40));
    let half = SandboxSize::parse("0.5").unwrap();
    assert_eq!(
        (half.cores(), half.memory()),
        ("0.5".into(), "1024Mi".into())
    );
    assert_eq!(SandboxSize::parse("3"), None);
    assert_eq!(SandboxSize::parse("lots"), None);
    assert_eq!(
        SandboxSize::reported("1000m", "2Gi"),
        Some(SandboxSize::standard(1000))
    );
    assert_eq!(SandboxSize::reported("1", "2048Mi"), None);
}

#[test]
fn sandbox_prices_add_cpu_and_memory_by_the_hour() {
    let mut cpu = item("", "Standard", "Azure Container Apps", 0.000024);
    cpu.meter = "Standard vCPU Active Usage".into();
    let mut memory = item("", "Standard", "Azure Container Apps", 0.000003);
    memory.meter = "Standard Memory Active Usage".into();
    assert_eq!(rates(&[cpu.clone()]), None);
    let rates = rates(&[cpu, memory]).unwrap();
    // 1 vCPU and 2 GiB: (0.000024 + 2 × 0.000003) × 3600.
    let price = rates.hourly(SandboxSize::standard(1000));
    assert!((price.0 - 0.108).abs() < 1e-9, "{price:?}");
}

mod spot;
