//! One reading of a host's CPU, memory, home disk and uptime, and the text a
//! remote shell prints for it. Linux and macOS hosts answer with different tools; both answers
//! parse into the same [`Sample`], counted the way `sysinfo` counts this
//! machine so a local and a remote host read alike.

use crate::{Error, Result};

/// Bytes of memory in use: total less available on Linux; active, wired,
/// compressed and speculative pages on macOS.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Memory {
    pub used: u64,
    pub total: u64,
}

impl Memory {
    pub fn percent(self) -> f32 {
        if self.total == 0 {
            return 0.;
        }
        (self.used as f64 / self.total as f64 * 100.).clamp(0., 100.) as f32
    }
}

/// The filesystem holding the user's home directory, where checkouts live:
/// space left to the user and the whole volume, in bytes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Disk {
    pub available: u64,
    pub total: u64,
}

impl Disk {
    pub fn used_percent(self) -> f32 {
        if self.total == 0 {
            return 0.;
        }
        let used = self.total.saturating_sub(self.available);
        (used as f64 / self.total as f64 * 100.).clamp(0., 100.) as f32
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct Sample {
    /// Share of all cores busy, 0..=100. None for a host's first sample when
    /// its counters need a previous one to compare with.
    pub cpu: Option<f32>,
    pub memory: Option<Memory>,
    pub cores: Option<u32>,
    /// 1, 5 and 15 minute load averages, where the OS keeps them.
    pub load: Option<[f32; 3]>,
    pub disk: Option<Disk>,
    /// Seconds since the host booted.
    pub uptime: Option<u64>,
}

/// Cumulative CPU time from Linux `/proc/stat`, in clock ticks.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Ticks {
    busy: u64,
    total: u64,
}

impl Ticks {
    fn usage_since(self, previous: Self) -> Option<f32> {
        let total = self.total.checked_sub(previous.total)?;
        let busy = self.busy.checked_sub(previous.busy)?;
        (total > 0).then(|| (busy as f64 / total as f64 * 100.).clamp(0., 100.) as f32)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Os {
    Linux,
    MacOs,
}

impl Os {
    pub fn from_uname(name: &str) -> Option<Self> {
        match name.trim() {
            "Linux" => Some(Self::Linux),
            "Darwin" => Some(Self::MacOs),
            _ => None,
        }
    }

    /// The step the remote shell runs for one sample. macOS has no cumulative
    /// CPU counters a shell can read, so `iostat` measures one second itself;
    /// Linux counters are compared with the previous sample instead. `df -P`
    /// keeps each filesystem on one line on both.
    pub fn command(self) -> &'static str {
        match self {
            Self::Linux => {
                "printf 'cores '; getconf _NPROCESSORS_ONLN; printf 'loadavg '; cat /proc/loadavg; \
                 head -n 1 /proc/stat; grep -E '^(MemTotal|MemAvailable):' /proc/meminfo; \
                 printf 'uptime '; cat /proc/uptime; printf 'disk '; df -Pk \"$HOME\" | tail -n 1"
            }
            Self::MacOs => {
                "printf 'cores '; sysctl -n hw.ncpu; printf 'memsize '; sysctl -n hw.memsize; \
                 vm_stat; printf 'boottime '; sysctl -n kern.boottime; printf 'now '; date +%s; \
                 printf 'disk '; df -Pk \"$HOME\" | tail -n 1; \
                 printf 'iostat '; iostat -n 0 -c 2 -w 1 | tail -n 1"
            }
        }
    }
}

/// A parsed remote answer, with the Linux counters the next one compares to.
#[derive(Debug, Default, PartialEq)]
pub(crate) struct Answer {
    pub sample: Sample,
    pub ticks: Option<Ticks>,
}

#[derive(Default)]
struct Fields {
    cores: Option<u32>,
    load: Option<[f32; 3]>,
    ticks: Option<Ticks>,
    idle: Option<f32>,
    mem_total_kb: Option<u64>,
    mem_available_kb: Option<u64>,
    memsize: Option<u64>,
    page_size: Option<u64>,
    pages: [Option<u64>; 4],
    disk: Option<Disk>,
    uptime: Option<u64>,
    boot: Option<u64>,
    now: Option<u64>,
}

/// The vm_stat counters macOS counts as memory in use.
const USED_PAGES: [&str; 4] = [
    "Pages active:",
    "Pages wired down:",
    "Pages occupied by compressor:",
    "Pages speculative:",
];

/// Parses what [`Os::command`] printed. `previous` is the last answer's
/// counters, from which Linux CPU usage is measured.
pub(crate) fn parse(text: &str, previous: Option<Ticks>) -> Result<Answer> {
    let mut fields = Fields::default();
    for line in text.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("cores ") {
            fields.cores = rest.trim().parse().ok().filter(|cores| *cores > 0);
        } else if let Some(rest) = line.strip_prefix("loadavg ") {
            fields.load = load(rest.split_whitespace());
        } else if let Some(rest) = line.strip_prefix("cpu ") {
            fields.ticks = ticks(rest);
        } else if let Some(rest) = line.strip_prefix("MemTotal:") {
            fields.mem_total_kb = kilobytes(rest);
        } else if let Some(rest) = line.strip_prefix("MemAvailable:") {
            fields.mem_available_kb = kilobytes(rest);
        } else if let Some(rest) = line.strip_prefix("memsize ") {
            fields.memsize = rest.trim().parse().ok();
        } else if let Some(rest) =
            line.strip_prefix("Mach Virtual Memory Statistics: (page size of ")
        {
            fields.page_size = rest
                .split_whitespace()
                .next()
                .and_then(|size| size.parse().ok());
        } else if let Some(rest) = line.strip_prefix("uptime ") {
            fields.uptime = rest
                .split_whitespace()
                .next()
                .and_then(|seconds| seconds.parse::<f64>().ok())
                .filter(|seconds| seconds.is_finite() && *seconds >= 0.)
                .map(|seconds| seconds as u64);
        } else if let Some(rest) = line.strip_prefix("boottime ") {
            fields.boot = boot_seconds(rest);
        } else if let Some(rest) = line.strip_prefix("now ") {
            fields.now = rest.trim().parse().ok();
        } else if let Some(rest) = line.strip_prefix("disk ") {
            fields.disk = disk(rest);
        } else if let Some(rest) = line.strip_prefix("iostat ") {
            // us sy id 1m 5m 15m
            let words: Vec<&str> = rest.split_whitespace().collect();
            fields.idle = words
                .get(2)
                .and_then(|idle| idle.parse::<f32>().ok())
                .filter(|idle| (0.0..=100.).contains(idle));
            fields.load = load(words.iter().skip(3).copied());
        } else if let Some(index) = USED_PAGES.iter().position(|name| line.starts_with(name)) {
            fields.pages[index] = line[USED_PAGES[index].len()..]
                .trim()
                .trim_end_matches('.')
                .parse()
                .ok();
        }
    }
    let memory = match (fields.mem_total_kb, fields.mem_available_kb) {
        (Some(total), Some(available)) => Some(Memory {
            used: total.saturating_sub(available).saturating_mul(1024),
            total: total.saturating_mul(1024),
        }),
        _ => fields
            .memsize
            .zip(fields.page_size)
            .and_then(|(total, page)| {
                let pages = fields
                    .pages
                    .iter()
                    .try_fold(0u64, |sum, pages| Some(sum.saturating_add((*pages)?)))?;
                Some(Memory {
                    used: pages.saturating_mul(page).min(total),
                    total,
                })
            }),
    }
    .filter(|memory| memory.total > 0);
    let cpu = match (fields.ticks, previous, fields.idle) {
        (Some(now), Some(previous), _) => now.usage_since(previous),
        (_, _, Some(idle)) => Some(100. - idle),
        _ => None,
    };
    if memory.is_none() && cpu.is_none() && fields.ticks.is_none() {
        return Err(Error::SystemLoadOutput);
    }
    Ok(Answer {
        sample: Sample {
            cpu,
            memory,
            cores: fields.cores,
            load: fields.load,
            disk: fields.disk,
            uptime: fields
                .uptime
                .or_else(|| fields.now?.checked_sub(fields.boot?)),
        },
        ticks: fields.ticks,
    })
}

fn load<'a>(mut words: impl Iterator<Item = &'a str>) -> Option<[f32; 3]> {
    let mut next = || {
        words
            .next()?
            .parse::<f32>()
            .ok()
            .filter(|value| value.is_finite() && *value >= 0.)
    };
    Some([next()?, next()?, next()?])
}

/// `user nice system idle iowait irq softirq steal …`; guest time is already
/// counted in user time, so only the first eight columns add up.
fn ticks(rest: &str) -> Option<Ticks> {
    let columns = rest
        .split_whitespace()
        .take(8)
        .map(str::parse::<u64>)
        .collect::<std::result::Result<Vec<_>, _>>()
        .ok()?;
    if columns.len() < 4 {
        return None;
    }
    let total = columns
        .iter()
        .fold(0u64, |sum, value| sum.saturating_add(*value));
    let idle = columns[3].saturating_add(columns.get(4).copied().unwrap_or(0));
    Some(Ticks {
        busy: total.saturating_sub(idle),
        total,
    })
}

/// `{ sec = 1712345678, usec = 123456 } Thu Apr  5 …`, from `kern.boottime`.
fn boot_seconds(rest: &str) -> Option<u64> {
    let after = rest.split_once("sec =")?.1;
    after
        .split(|c: char| !c.is_ascii_digit())
        .find(|digits| !digits.is_empty())?
        .parse()
        .ok()
}

/// One `df -Pk` row: `filesystem 1024-blocks used available capacity mount`.
/// Both the filesystem name and the mount point may contain spaces, so the
/// row is read around its capacity: the first `NN%` word that follows three
/// numbers, which are the size, the used space, and the available space.
fn disk(rest: &str) -> Option<Disk> {
    let words: Vec<&str> = rest.split_whitespace().collect();
    let capacity = |word: &str| {
        word.strip_suffix('%')
            .is_some_and(|digits| !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit()))
    };
    let (total, available) = words.windows(4).find_map(|window| {
        let [total, used, available, percent] = window else {
            return None;
        };
        if !capacity(percent) {
            return None;
        }
        used.parse::<u64>().ok()?;
        Some((total.parse::<u64>().ok()?, available.parse::<u64>().ok()?))
    })?;
    let total = total.checked_mul(1024)?;
    let available = available.checked_mul(1024)?;
    (total > 0).then_some(Disk {
        available: available.min(total),
        total,
    })
}

fn kilobytes(rest: &str) -> Option<u64> {
    rest.split_whitespace().next()?.parse().ok()
}
