//! This computer's CPU and memory, which the phone shows above its
//! workspaces. A thread of its own samples them through `sysinfo` and
//! publishes whole percentages and a load average in tenths, so a home goes
//! out again only when a figure moves by a step, and at most once per
//! [`INTERVAL`]. The core count and load average let a phone rank computers
//! by spare cores, as the desktop's dispatch does.

use serde::{Deserialize, Serialize};
use std::{thread, time::Duration};
use tokio::sync::watch;

/// How often the figures are read.
const INTERVAL: Duration = Duration::from_secs(5);

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub(crate) struct Load {
    /// Logical cores; absent when none were read.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) cores: Option<u32>,
    /// Share of all cores busy, 0..=100.
    pub(crate) cpu: u8,
    /// The 5 minute load average in tenths, where the OS keeps one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) load5_tenths: Option<u16>,
    /// Share of memory in use, 0..=100.
    pub(crate) memory: u8,
}

impl Load {
    /// From a CPU share in percent, memory in bytes, the core count, and
    /// the 5 minute load average; None without a memory total.
    pub(crate) fn new(
        cpu: f32,
        used: u64,
        total: u64,
        cores: usize,
        load5: Option<f64>,
    ) -> Option<Self> {
        (total > 0).then(|| Self {
            cores: u32::try_from(cores).ok().filter(|cores| *cores > 0),
            cpu: whole(f64::from(cpu)),
            load5_tenths: load5.map(tenths),
            memory: whole(used as f64 / total as f64 * 100.),
        })
    }
}

fn whole(percent: f64) -> u8 {
    // In range after the clamp, and NaN casts to 0.
    percent.clamp(0., 100.).round() as u8
}

fn tenths(load: f64) -> u16 {
    // Saturates at the top of the range, and NaN casts to 0.
    (load.max(0.) * 10.).round() as u16
}

/// Starts the sampling thread. The thread ends once every receiver is gone;
/// until its first reading, and when it could not start, the load is None.
pub(crate) fn start() -> watch::Receiver<Option<Load>> {
    let (sender, receiver) = watch::channel(None);
    let spawned = thread::Builder::new()
        .name("herdr-remote-load".into())
        .spawn(move || {
            let mut system = sysinfo::System::new();
            // CPU use is measured between two refreshes.
            system.refresh_cpu_usage();
            thread::sleep(sysinfo::MINIMUM_CPU_UPDATE_INTERVAL);
            while !sender.is_closed() {
                system.refresh_cpu_usage();
                system.refresh_memory();
                let load = Load::new(
                    system.global_cpu_usage(),
                    system.used_memory(),
                    system.total_memory(),
                    system.cpus().len(),
                    load5(),
                );
                sender.send_if_modified(|current| {
                    let changed = *current != load;
                    *current = load;
                    changed
                });
                thread::sleep(INTERVAL);
            }
        });
    if let Err(error) = spawned {
        tracing::warn!(%error, "Could not start reading this computer's load");
    }
    receiver
}

/// The load average, which Windows does not keep: `sysinfo` reports zero
/// there, which would read as an idle computer.
fn load5() -> Option<f64> {
    (!cfg!(windows)).then(|| sysinfo::System::load_average().five)
}

#[cfg(test)]
mod tests;
