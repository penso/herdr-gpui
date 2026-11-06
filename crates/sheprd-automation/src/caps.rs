//! The limits a script declares and a person approves with it. The runner
//! enforces them; the host's own caps sit on top.

use crate::{args::Site, schedule::format_duration};
use mlua::Table;
use serde::Serialize;
use std::{fmt, time::Duration};

pub const MAX_AGENTS_PER_RUN: u64 = 16;
pub const MAX_TOKENS_PER_DAY: u64 = 1_000_000_000;
pub const MAX_WALL_TIME: Duration = Duration::from_secs(24 * 3_600);

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Caps {
    pub agents_per_run: u32,
    pub tokens_per_day: u64,
    #[serde(rename = "wall_time_seconds", serialize_with = "seconds")]
    pub wall_time: Duration,
}

impl Caps {
    pub(crate) fn read(table: &Table) -> mlua::Result<Self> {
        let site = Site {
            function: "herdr.caps",
        };
        site.only(table, &["agents_per_run", "tokens_per_day", "wall_time"])?;
        let agents = site.whole(
            table,
            "agents_per_run",
            0,
            MAX_AGENTS_PER_RUN,
            "a whole number from 0 to 16",
        )?;
        let tokens_per_day = site.whole(
            table,
            "tokens_per_day",
            0,
            MAX_TOKENS_PER_DAY,
            "a whole number from 0 to 1,000,000,000",
        )?;
        let wall_time = match table.raw_get::<mlua::Value>("wall_time")? {
            mlua::Value::Nil => return Err(site.refuse(crate::Problem::Missing("wall_time"))),
            value => site.duration(
                value,
                "wall_time",
                Duration::from_secs(60),
                MAX_WALL_TIME,
                "between 1m and 1d",
            )?,
        };
        Ok(Self {
            agents_per_run: u32::try_from(agents).unwrap_or(u32::MAX),
            tokens_per_day,
            wall_time,
        })
    }
}

fn seconds<S: serde::Serializer>(duration: &Duration, serializer: S) -> Result<S::Ok, S::Error> {
    serializer.serialize_u64(duration.as_secs())
}

impl fmt::Display for Caps {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let agents = match self.agents_per_run {
            1 => "1 agent".to_owned(),
            count => format!("{count} agents"),
        };
        write!(
            f,
            "{agents} per run · {} tokens/day · {} per run",
            group(self.tokens_per_day),
            format_duration(self.wall_time)
        )
    }
}

/// `2000000` as `2,000,000`.
fn group(value: u64) -> String {
    let digits = value.to_string();
    let mut grouped = String::new();
    for (index, digit) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            grouped.push(',');
        }
        grouped.push(digit);
    }
    grouped
}
