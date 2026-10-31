//! How far a build is, for a progress bar. Local engines number their steps
//! in their output: BuildKit's plain progress says `#7 [3/5] RUN …`, Podman
//! (Buildah) `STEP 3/5: RUN …`, and Docker's classic builder `Step 3/5 : RUN
//! …`. A provider building elsewhere reports only its state, and the bar
//! then shows activity without a fraction.

/// A build's latest progress.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Progress {
    /// The step running and how many there are, when the builder says.
    pub step: Option<(u32, u32)>,
    /// What the builder last said, for the line under the bar.
    pub status: String,
}

impl Progress {
    /// How far along the build is, if it can be told.
    pub fn fraction(&self) -> Option<f32> {
        let (step, total) = self.step?;
        (total > 0).then(|| (step.saturating_sub(1) as f32 / total as f32).clamp(0.0, 1.0))
    }

    /// Take one line of a local builder's output into account.
    pub fn line(&mut self, line: &str) {
        let line = line.trim();
        if let Some((step, instruction)) = numbered(line) {
            self.step = Some(step);
            self.status = shorten(instruction);
        } else if !line.is_empty() {
            self.status = shorten(line);
        }
    }
}

/// The step `[i/n]` or `STEP i/n:` a builder line starts, with its
/// instruction.
fn numbered(line: &str) -> Option<((u32, u32), &str)> {
    // BuildKit: `#7 [stage 3/5] RUN …`.
    if line.starts_with('#')
        && let Some(open) = line.find('[')
        && let Some(close) = line[open..].find(']')
    {
        let inside = &line[open + 1..open + close];
        let fraction = inside.rsplit(' ').next()?;
        return Some((ratio(fraction)?, line[open + close + 1..].trim()));
    }
    // Buildah: `STEP 3/5: RUN …`, maybe after a stage `[1/2] `; Docker's
    // classic builder: `Step 3/5 : RUN …`.
    let at = line.find("STEP ").or_else(|| line.find("Step "))?;
    let rest = &line[at + 5..];
    let (fraction, instruction) = rest.split_once(':')?;
    Some((ratio(fraction.trim())?, instruction.trim()))
}

fn ratio(text: &str) -> Option<(u32, u32)> {
    let (step, total) = text.split_once('/')?;
    Some((step.parse().ok()?, total.parse().ok()?))
}

fn shorten(text: &str) -> String {
    const LIMIT: usize = 120;
    let text: String = text.chars().filter(|c| !c.is_control()).collect();
    if text.chars().count() <= LIMIT {
        return text;
    }
    let mut short: String = text.chars().take(LIMIT - 1).collect();
    short.push('…');
    short
}

#[cfg(test)]
mod tests;
