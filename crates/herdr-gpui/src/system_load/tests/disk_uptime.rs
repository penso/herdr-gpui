use super::*;
use crate::system_load::{
    home_volume,
    render::{storage, uptime},
    sample::Disk,
};
use std::path::Path;

const LINUX_DISK: &str = "uptime 3226283.57 12345678.90
disk /dev/nvme0n1p2 976284628 763817104 162789060      83% /
";

const MACOS_DISK: &str = "boottime { sec = 1700000000, usec = 52344 } Tue Nov 14 22:13:20 2023
now 1700300000
disk /dev/disk3s1s1 971350180 10560712 399446448 3% /
";

#[test]
fn linux_reports_uptime_and_the_home_volume() {
    let answer = parse(&format!("{LINUX}{LINUX_DISK}"), None).unwrap();
    assert_eq!(answer.sample.uptime, Some(3_226_283));
    assert_eq!(
        answer.sample.disk,
        Some(Disk {
            available: 162_789_060 * 1024,
            total: 976_284_628 * 1024,
        })
    );
}

#[test]
fn macos_uptime_is_measured_from_boot_time() {
    let answer = parse(&format!("{MACOS}{MACOS_DISK}"), None).unwrap();
    assert_eq!(answer.sample.uptime, Some(300_000));
    let disk = answer.sample.disk.unwrap();
    assert_eq!(disk.available, 399_446_448 * 1024);
    // APFS volumes share their container, so used is total less available.
    assert!(
        (disk.used_percent() - 58.88).abs() < 0.01,
        "{}",
        disk.used_percent()
    );
}

#[test]
fn a_failed_df_or_clock_leaves_only_those_fields_unknown() {
    for broken in [
        "disk Filesystem 1024-blocks Used Available Capacity Mounted on\n",
        "disk \n",
        "disk /dev/sda1 0 0 0 0% /\n",
        "disk /dev/sda1 x y z 1% /\n",
        "uptime -5 3\nboottime garbage\nnow 12\n",
        "boottime { sec = 200 }\nnow 100\n",
    ] {
        let answer = parse(&format!("{LINUX}{broken}"), None).unwrap();
        assert_eq!(answer.sample.disk, None, "{broken:?}");
        assert_eq!(answer.sample.uptime, None, "{broken:?}");
        assert!(answer.sample.memory.is_some(), "the rest still parses");
    }
}

#[test]
fn spaces_in_the_filesystem_or_the_mount_keep_the_reading() {
    let expected = Some(Disk {
        available: 512 * 1024,
        total: 2048 * 1024,
    });
    for row in [
        "//nas/My Share 2048 1024 512 67% /",
        "/dev/disk4s1 2048 1024 512 67% /Volumes/My Drive",
        // A number in the name is not taken for a column.
        "//nas/Share 2 2048 1024 512 67% /Volumes/Backup 2024",
    ] {
        let answer = parse(&format!("{LINUX}disk {row}\n"), None).unwrap();
        assert_eq!(answer.sample.disk, expected, "{row}");
    }
}

#[test]
fn the_home_volume_is_the_deepest_mount_holding_home() {
    let mounts = [
        Path::new("/"),
        Path::new("/home"),
        Path::new("/home2"),
        Path::new("/boot"),
    ];
    assert_eq!(home_volume(Path::new("/home/penso"), mounts), Some(1));
    assert_eq!(home_volume(Path::new("/Users/penso"), mounts), Some(0));
    assert_eq!(
        home_volume(Path::new("/Users/penso"), [Path::new("/Volumes/USB")]),
        None
    );
}

#[test]
fn sizes_and_uptimes_read_short() {
    assert_eq!(storage(7 << 29), "3.5 GB");
    assert_eq!(storage(381 << 30), "381 GB");
    assert_eq!(storage(1229 << 30), "1.2 TB");
    assert_eq!(uptime(116 * 86_400 + 59 * 60), "116d 0h");
    assert_eq!(uptime(3 * 3600 + 12 * 60 + 5), "3h 12m");
    assert_eq!(uptime(59), "0m");
    assert_eq!(
        Disk {
            available: 1,
            total: 0
        }
        .used_percent(),
        0.
    );
    assert_eq!(
        Disk {
            available: 5,
            total: 4
        }
        .used_percent(),
        0.
    );
    assert_eq!(
        Disk {
            available: 1,
            total: 4
        }
        .used_percent(),
        75.
    );
}

#[test]
fn this_machine_reports_its_uptime_and_home_volume() {
    let mut source = Source::open(&Host::Local).unwrap();
    let sample = source.sample().unwrap();
    assert!(sample.uptime.is_some_and(|seconds| seconds > 0));
    let disk = sample.disk.expect("the home directory lies on some volume");
    assert!(disk.total > 0 && disk.available <= disk.total, "{disk:?}");
}
