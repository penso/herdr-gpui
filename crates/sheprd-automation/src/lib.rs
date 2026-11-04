//! Automation scripts for Herdr: sandboxed Luau that declares triggers and
//! caps, and calls steps from a closed catalog. Steps are a script's only
//! effects, and what a script cannot know (the date, what a step printed)
//! reaches it only as tokens, so [`Plan::build`] can show every path it can
//! take before anyone approves it, and [`Walker`] keeps real runs on it.
//!
//! ```lua
//! herdr.caps { agents_per_run = 1, tokens_per_day = 2_000_000, wall_time = "3h" }
//!
//! herdr.at("03:00", { tz = "Europe/Paris" }, function(fire)
//!   pull("main")
//!   run("validation")
//!   agent { harness = "claude", prompt = "..." }
//! end)
//! ```

#[cfg(feature = "engine")]
mod args;
#[cfg(feature = "engine")]
mod caps;
mod error;
#[cfg(feature = "engine")]
mod plan;
pub mod report;
#[cfg(feature = "engine")]
mod schedule;
#[cfg(feature = "engine")]
mod script;
#[cfg(feature = "engine")]
mod source;
#[cfg(feature = "engine")]
mod step;
#[cfg(feature = "engine")]
mod tokens;
#[cfg(feature = "engine")]
mod vm;

#[cfg(feature = "engine")]
pub use args::{invisible, visible};
#[cfg(feature = "engine")]
pub use caps::Caps;
#[cfg(feature = "engine")]
pub use error::{Budget, Problem, Refusal};
pub use error::{Error, Result};
#[cfg(feature = "engine")]
pub use plan::{Failure, MAX_PATHS, Plan, PlannedStep, TriggerPlan, Walker};
#[cfg(feature = "engine")]
pub use schedule::{
    ClockTime, Cron, Days, Due, LATE_AFTER, Schedule, Zone, due, format_duration, parse_duration,
};
#[cfg(feature = "engine")]
pub use script::{End, Script, Trigger};
#[cfg(feature = "engine")]
pub use source::{MAX_SCRIPT_BYTES, Source};
#[cfg(feature = "engine")]
pub use step::{Call, Kind, Step, Verdict};
#[cfg(feature = "engine")]
pub use tokens::{DATE, Resolved, TIME, Values};
#[cfg(feature = "engine")]
pub use vm::Limits;

#[cfg(all(test, feature = "engine"))]
#[allow(clippy::unwrap_used)]
mod tests;
