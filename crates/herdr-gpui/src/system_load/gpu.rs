//! A host's graphics cards: how busy they are and how much of their own
//! memory is in use. NVIDIA cards answer through `nvidia-smi`, AMD cards
//! through the `amdgpu` driver's sysfs files, and Apple GPUs through `ioreg`.
//! A remote shell prints one `gpu-*` line per card from [`LINUX_COMMAND`] or
//! [`MACOS_COMMAND`]; this machine reads the same sources itself, so a local
//! and a remote host read alike. Intel cards report no usage without root
//! and are left out.

use super::sample::Memory;
use std::time::Duration;

/// Several cards read as one: their mean busy share and their memory summed.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct Gpu {
    /// Share of the time the cards were busy, 0..=100.
    pub busy: Option<f32>,
    /// Dedicated video memory. None on Apple GPUs, which share system memory.
    pub memory: Option<Memory>,
    pub count: u32,
}

impl Gpu {
    /// None when no card answered.
    pub fn combine(cards: impl IntoIterator<Item = Card>) -> Option<Self> {
        let (mut count, mut busy_sum, mut busy_count) = (0u32, 0f32, 0u32);
        let mut memory: Option<Memory> = None;
        for card in cards {
            count = count.saturating_add(1);
            if let Some(busy) = card.busy {
                busy_sum += busy;
                busy_count += 1;
            }
            if let Some(card) = card.memory {
                let sum = memory.get_or_insert_with(Memory::default);
                sum.used = sum.used.saturating_add(card.used);
                sum.total = sum.total.saturating_add(card.total);
            }
        }
        (count > 0).then(|| Self {
            busy: (busy_count > 0).then(|| (busy_sum / busy_count as f32).clamp(0., 100.)),
            memory: memory.filter(|memory| memory.total > 0),
            count,
        })
    }
}

/// One card's reading.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct Card {
    pub busy: Option<f32>,
    pub memory: Option<Memory>,
}

/// What `nvidia-smi` is asked for, one card per line, in MiB.
const NVIDIA_QUERY: &str =
    "--query-gpu=utilization.gpu,memory.used,memory.total --format=csv,noheader,nounits";

/// Prints a `gpu-amd` line per AMD card and a `gpu-nvidia` line per NVIDIA
/// card. `timeout` keeps a wedged driver from stalling the whole sample.
pub(super) const LINUX_COMMAND: &str = "for herdr_g in /sys/class/drm/card*/device/gpu_busy_percent; do \
     [ -r \"$herdr_g\" ] || continue; herdr_d=${herdr_g%/gpu_busy_percent}; \
     printf 'gpu-amd %s %s %s\\n' \"$(cat \"$herdr_g\" 2>/dev/null)\" \
     \"$(cat \"$herdr_d/mem_info_vram_used\" 2>/dev/null)\" \
     \"$(cat \"$herdr_d/mem_info_vram_total\" 2>/dev/null)\"; done; \
     if command -v nvidia-smi >/dev/null 2>&1; then herdr_t=; \
     command -v timeout >/dev/null 2>&1 && herdr_t='timeout 3'; \
     $herdr_t nvidia-smi --query-gpu=utilization.gpu,memory.used,memory.total \
     --format=csv,noheader,nounits 2>/dev/null | sed 's/^/gpu-nvidia /'; fi";

/// Prints a `gpu-apple` line per GPU with its utilization.
pub(super) const MACOS_COMMAND: &str = "ioreg -r -d 1 -w 0 -c IOAccelerator 2>/dev/null | \
     grep -o '\"Device Utilization %\"=[0-9]*' | sed 's/^.*=/gpu-apple /'";

/// One `gpu-*` line of a remote answer, or None for any other line.
pub(super) fn parse_line(line: &str) -> Option<Card> {
    if let Some(rest) = line.strip_prefix("gpu-nvidia ") {
        nvidia(rest)
    } else if let Some(rest) = line.strip_prefix("gpu-amd ") {
        let mut words = rest.split_whitespace();
        let busy = percent(words.next()?)?;
        let used = words.next().and_then(|used| used.parse().ok());
        let total = words.next().and_then(|total| total.parse().ok());
        Some(Card {
            busy: Some(busy),
            memory: memory(used, total),
        })
    } else {
        let rest = line.strip_prefix("gpu-apple ")?;
        Some(Card {
            busy: Some(percent(rest.trim())?),
            memory: None,
        })
    }
}

/// `45, 9235, 24576`: busy share, then used and total memory in MiB. A card
/// that cannot report a figure prints `[N/A]` there.
fn nvidia(row: &str) -> Option<Card> {
    let mut columns = row.split(',').map(str::trim);
    let busy = columns.next().and_then(percent);
    let mib = |column: Option<&str>| {
        column
            .and_then(|value| value.parse::<u64>().ok())
            .and_then(|value| value.checked_mul(1 << 20))
    };
    let memory = memory(mib(columns.next()), mib(columns.next()));
    (busy.is_some() || memory.is_some()).then_some(Card { busy, memory })
}

fn percent(text: &str) -> Option<f32> {
    text.parse::<f32>()
        .ok()
        .filter(|value| value.is_finite() && *value >= 0.)
        .map(|value| value.min(100.))
}

fn memory(used: Option<u64>, total: Option<u64>) -> Option<Memory> {
    let (used, total) = (used?, total?);
    (total > 0).then(|| Memory {
        used: used.min(total),
        total,
    })
}

/// How long a local tool may take before it is given up on.
const TOOL_TIMEOUT: Duration = Duration::from_secs(3);
/// Output a tool may print. `nvidia-smi` prints a few dozen bytes a card,
/// but `ioreg` prints every accelerator property, already ~43 KB for one
/// Apple GPU; a longer answer closes the pipe and the tool is given up on.
pub(super) const TOOL_LIMIT: usize = 1024 * 1024;

/// This machine's cards. A tool that is missing or overruns once is not run
/// again for this worker, so a machine without a GPU spends nothing on it.
pub(super) struct Local {
    nvidia: bool,
    #[cfg(target_os = "macos")]
    ioreg: bool,
}

impl Local {
    pub fn open() -> Self {
        Self {
            nvidia: !cfg!(target_os = "macos"),
            #[cfg(target_os = "macos")]
            ioreg: true,
        }
    }

    /// Runs on the load worker's thread: it reads sysfs and waits for tools.
    pub fn sample(&mut self) -> Option<Gpu> {
        let mut cards = Vec::new();
        #[cfg(target_os = "linux")]
        cards.extend(amd_cards());
        if self.nvidia {
            match tool("nvidia-smi", NVIDIA_QUERY.split(' '), TOOL_TIMEOUT) {
                Some(text) => cards.extend(text.lines().filter_map(nvidia)),
                None => self.nvidia = false,
            }
        }
        #[cfg(target_os = "macos")]
        if self.ioreg {
            match tool(
                "ioreg",
                ["-r", "-d", "1", "-w", "0", "-c", "IOAccelerator"],
                TOOL_TIMEOUT,
            ) {
                Some(text) => cards.extend(apple_cards(&text)),
                None => self.ioreg = false,
            }
        }
        Gpu::combine(cards)
    }
}

/// `"Device Utilization %"=12` in each accelerator's performance statistics.
#[cfg(any(target_os = "macos", test))]
pub(super) fn apple_cards(text: &str) -> impl Iterator<Item = Card> + '_ {
    const KEY: &str = "\"Device Utilization %\"=";
    text.match_indices(KEY).filter_map(|(start, _)| {
        let digits: String = text[start + KEY.len()..]
            .chars()
            .take_while(char::is_ascii_digit)
            .collect();
        Some(Card {
            busy: Some(percent(&digits)?),
            memory: None,
        })
    })
}

#[cfg(target_os = "linux")]
fn amd_cards() -> Vec<Card> {
    let Ok(entries) = std::fs::read_dir("/sys/class/drm") else {
        return Vec::new();
    };
    entries
        .filter_map(|entry| {
            let entry = entry.ok()?;
            // `card1`, not its connectors such as `card1-DP-1`.
            let name = entry.file_name();
            let number = name.to_str()?.strip_prefix("card")?;
            if number.is_empty() || !number.bytes().all(|b| b.is_ascii_digit()) {
                return None;
            }
            let device = entry.path().join("device");
            let read = |file: &str| std::fs::read_to_string(device.join(file)).ok();
            let busy = percent(read("gpu_busy_percent")?.trim())?;
            let bytes = |file: &str| read(file)?.trim().parse::<u64>().ok();
            Some(Card {
                busy: Some(busy),
                memory: memory(bytes("mem_info_vram_used"), bytes("mem_info_vram_total")),
            })
        })
        .collect()
}

/// What `program` printed, or None when it could not start, failed, printed
/// more than [`TOOL_LIMIT`], or was still running after `timeout`. A reader
/// thread drains its output while this one waits, so a long answer cannot
/// fill the pipe and stall it; the deadline covers the process, not only its
/// output, so a tool that closes stdout and keeps running is still ended.
pub(super) fn tool<'a>(
    program: &str,
    args: impl IntoIterator<Item = &'a str>,
    timeout: Duration,
) -> Option<String> {
    use std::{io::Read, process::Stdio, sync::mpsc, time::Instant};
    let deadline = Instant::now() + timeout;
    let mut child = herdr_client::new_command(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let stdout = child.stdout.take();
    let (sender, read) = mpsc::sync_channel(1);
    let reader = std::thread::Builder::new()
        .name("herdr-gpu-tool".into())
        .spawn(move || {
            let mut text = String::new();
            // One byte past the cap tells a full answer from a cut one.
            let read = stdout.is_some_and(|stdout| {
                stdout
                    .take(TOOL_LIMIT as u64 + 1)
                    .read_to_string(&mut text)
                    .is_ok()
            });
            let _ = sender.send(read.then_some(text));
        });
    let text = reader
        .ok()
        .and_then(|_| read.recv_timeout(timeout).ok())
        .flatten()
        .filter(|text| text.len() <= TOOL_LIMIT);
    // Polled on the load worker's thread, which has nothing else to do
    // until the tool has answered.
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) if text.is_some() && Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(10));
            }
            _ => {
                // Ends the child, and with it the reader's pipe.
                let _ = child.kill();
                let _ = child.wait();
                break None;
            }
        }
    };
    text.filter(|_| status.is_some_and(|status| status.success()))
}
