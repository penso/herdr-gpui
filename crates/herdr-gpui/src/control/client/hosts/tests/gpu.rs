use super::*;

fn load(gpu_count: Option<u32>, gpu_percent: Option<f32>, memory: Option<(u64, u64)>) -> Reply {
    Reply::System {
        load: Some(Box::new(LoadInfo {
            host: "Local".into(),
            cpu_percent: Some(25.),
            memory_used: None,
            memory_total: None,
            cores: None,
            load: None,
            uptime_seconds: None,
            disk_available: None,
            disk_total: None,
            gpu_count,
            gpu_percent,
            gpu_memory_used: memory.map(|(used, _)| used),
            gpu_memory_total: memory.map(|(_, total)| total),
        })),
        note: None,
    }
}

#[test]
fn system_text_lists_the_gpu_and_its_video_memory() {
    assert_eq!(
        text::system(&load(Some(1), Some(45.), Some((9 << 30, 24 << 30)))),
        "Local\n  CPU     25%\n  GPU     45%\n  VRAM    9.0 GiB of 24.0 GiB"
    );
    // An Apple GPU shares system memory, so it has no VRAM line.
    assert_eq!(
        text::system(&load(Some(2), Some(10.), None)),
        "Local\n  CPU     25%\n  GPU     10% across 2 cards"
    );
    assert_eq!(
        text::system(&load(None, None, None)),
        "Local\n  CPU     25%"
    );
}

#[test]
fn gpu_fields_are_left_out_of_json_for_hosts_without_one() {
    let without = serde_json::to_string(&load(None, None, None)).unwrap();
    assert!(!without.contains("gpu"), "{without}");
    let with = serde_json::to_string(&load(Some(1), Some(45.), None)).unwrap();
    assert!(with.contains("\"gpu_percent\":45.0"), "{with}");
    // Without the fields, as a build before GPUs were sampled sends, it reads.
    let parsed: Reply = serde_json::from_str(&without).unwrap();
    assert_eq!(parsed, load(None, None, None));
}
