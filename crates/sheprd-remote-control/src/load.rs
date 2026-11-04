//! This computer's CPU and memory, which the phone shows above its
//! workspaces. A thread of its own samples them through `sysinfo` and
//! publishes whole percentages, so a home goes out again only when a figure
//! moves by a point, and at most once per [`INTERVAL`].

use serde::{Deserialize, Serialize};
use std::{thread, time::Duration};
use tokio::sync::watch;

/// How often the figures are read.
const INTERVAL: Duration = Duration::from_secs(5);

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub(crate) struct Load {
    /// Share of all cores busy, 0..=100.
    pub(crate) cpu: u8,
    /// Share of memory in use, 0..=100.
    pub(crate) memory: u8,
}

impl Load {
    /// From a CPU share in percent and memory in bytes; None without a
    /// memory total.
    pub(crate) fn new(cpu: f32, used: u64, total: u64) -> Option<Self> {
        (total > 0).then(|| Self {
            cpu: whole(f64::from(cpu)),
            memory: whole(used as f64 / total as f64 * 100.),
        })
    }
}

fn whole(percent: f64) -> u8 {
    // In range after the clamp, and NaN casts to 0.
    percent.clamp(0., 100.).round() as u8
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

#[cfg(test)]
mod tests;
