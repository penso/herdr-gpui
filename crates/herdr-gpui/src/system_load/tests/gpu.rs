use super::*;
use crate::system_load::gpu::{Card, Gpu, apple_cards, parse_line};

const GIB: u64 = 1 << 30;

#[test]
fn nvidia_rows_read_busy_share_and_video_memory_in_mib() {
    let card = parse_line("gpu-nvidia 45, 9216, 24576").unwrap();
    assert_eq!(card.busy, Some(45.));
    assert_eq!(
        card.memory,
        Some(Memory {
            used: 9 * GIB,
            total: 24 * GIB,
        })
    );
    // A card without a figure prints `[N/A]` there; the rest still reads.
    let card = parse_line("gpu-nvidia [N/A], 512, 4096").unwrap();
    assert_eq!(card.busy, None);
    assert!(card.memory.is_some());
    assert_eq!(parse_line("gpu-nvidia [N/A], [N/A], [N/A]"), None);
}

#[test]
fn amd_and_apple_lines_parse_and_bad_figures_are_dropped() {
    assert_eq!(
        parse_line("gpu-amd 30 1073741824 8589934592"),
        Some(Card {
            busy: Some(30.),
            memory: Some(Memory {
                used: GIB,
                total: 8 * GIB,
            }),
        })
    );
    // An APU without video memory files still reports how busy it is.
    assert_eq!(
        parse_line("gpu-amd 7  "),
        Some(Card {
            busy: Some(7.),
            memory: None,
        })
    );
    assert_eq!(parse_line("gpu-amd  "), None);
    assert_eq!(parse_line("gpu-amd -4 1 2"), None);
    assert_eq!(parse_line("gpu-apple 250").unwrap().busy, Some(100.));
    assert_eq!(parse_line("gpu-apple NaN"), None);
    assert_eq!(parse_line("cores 8"), None);
    // Used memory never exceeds the card's total.
    let card = parse_line("gpu-amd 1 9 4").unwrap();
    assert_eq!(card.memory.unwrap().used, 4);
}

#[test]
fn ioreg_statistics_name_each_accelerators_utilization() {
    let text = r#"+-o AGXAcceleratorG14X  <class AGXAcceleratorG14X>
    "PerformanceStatistics" = {"In use system memory"=123,"Device Utilization %"=37,"Renderer Utilization %"=30}
+-o Other  <class IOAccelerator>
    "PerformanceStatistics" = {"Device Utilization %"=5}
"#;
    let busy: Vec<_> = apple_cards(text).map(|card| card.busy).collect();
    assert_eq!(busy, [Some(37.), Some(5.)]);
}

#[test]
fn several_cards_combine_into_a_mean_share_and_summed_memory() {
    assert_eq!(Gpu::combine([]), None);
    let gpu = Gpu::combine([
        Card {
            busy: Some(80.),
            memory: Some(Memory {
                used: 2 * GIB,
                total: 8 * GIB,
            }),
        },
        Card {
            busy: Some(20.),
            memory: None,
        },
        Card {
            busy: None,
            memory: Some(Memory {
                used: 6 * GIB,
                total: 8 * GIB,
            }),
        },
    ])
    .unwrap();
    assert_eq!(gpu.count, 3);
    assert_eq!(gpu.busy, Some(50.));
    assert_eq!(gpu.memory.unwrap().percent(), 50.);
}

#[test]
fn a_remote_answer_carries_its_gpu_lines() {
    let answer = parse(
        &format!("{LINUX}gpu-nvidia 90, 1024, 4096\ngpu-nvidia 10, 1024, 4096\n"),
        None,
    )
    .unwrap();
    let gpu = answer.sample.gpu.unwrap();
    assert_eq!((gpu.count, gpu.busy), (2, Some(50.)));
    assert_eq!(gpu.memory.unwrap().percent(), 25.);
    assert_eq!(parse(LINUX, None).unwrap().sample.gpu, None);
    // GPU lines alone are not a load sample.
    assert!(parse("gpu-nvidia 90, 1024, 4096\n", None).is_err());
    assert!(Os::Linux.command(true).contains("nvidia-smi"));
    assert!(Os::MacOs.command(true).contains("IOAccelerator"));
    // With the GPU hidden, no GPU tool runs on the host.
    assert!(!Os::Linux.command(false).contains("gpu-"));
    assert!(!Os::MacOs.command(false).contains("gpu-"));
}

#[test]
fn gpu_history_follows_the_cards_and_clears_without_them() {
    let gpu = |busy| {
        Some(Gpu {
            busy: Some(busy),
            memory: None,
            count: 1,
        })
    };
    let mut reading = Reading::default();
    for busy in 0..HISTORY + 3 {
        reading.apply(Ok(Sample {
            gpu: gpu(busy as f32),
            ..Sample::default()
        }));
    }
    assert_eq!(reading.gpu_history().len(), HISTORY);
    assert_eq!(reading.gpu_history().last(), Some((HISTORY + 2) as f32));
    // CPU history is separate and empty here.
    assert_eq!(reading.history().len(), 0);
    reading.apply(Ok(Sample::default()));
    assert_eq!(reading.gpu_history().len(), 0);
}

/// The remote command, run by a local `sh`, finds the same cards this
/// machine reads itself: none on a machine without a GPU.
#[cfg(target_os = "linux")]
#[test]
fn the_remote_command_and_this_machine_find_the_same_cards() {
    let mut command = std::process::Command::new("/bin/sh");
    command
        .arg("-s")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null());
    let mut shell = crate::usage::Shell::start(command).unwrap();
    let output = shell
        .run(
            crate::system_load::gpu::LINUX_COMMAND,
            Duration::from_secs(10),
        )
        .unwrap();
    let remote = Gpu::combine(output.stdout.lines().filter_map(parse_line));
    let local = crate::system_load::gpu::Local::open().sample();
    assert_eq!(
        remote.map(|gpu| gpu.count),
        local.map(|gpu| gpu.count),
        "{}",
        output.stdout
    );
}
