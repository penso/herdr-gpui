//! The wheel against a simulated daemon that serves requests in order,
//! late and several to a surface, prints output while scrolled back, and
//! is scrolled by the keyboard, checking the bookkeeping after every step.
use super::*;

/// A small seeded generator, so a failure names the seed that replays it.
struct Rng(u64);

impl Rng {
    fn below(&mut self, n: u64) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0 % n
    }
}

/// The daemon's view of the pane: the row at its top, its offset, and
/// whether every row reads alike, so the picture matches any shift.
#[derive(Clone, Copy)]
struct Daemon {
    top: u32,
    offset: u64,
    blank: bool,
}

impl Daemon {
    fn surface(self) -> Arc<PaneSurfaceFrame> {
        let mut surface = surface(self.top, self.offset);
        if self.blank {
            for cell in &mut Arc::make_mut(&mut surface).frame.cells {
                cell.symbol = " ".into();
            }
        }
        surface
    }

    /// Scrolling `lines` into history moves the content down as many rows.
    fn scroll(&mut self, lines: i64) {
        self.offset = self.offset.saturating_add_signed(lines);
        self.top = self.top.saturating_add_signed(-lines as i32);
    }
}

/// Runs one seed, returning how many steps were checked and how many were
/// exempt as ambiguous.
fn run(seed: u64) -> (u32, u32) {
    let mut rng = Rng(seed);
    let mut scroll = SmoothScroll::default();
    let mut daemon = Daemon {
        top: 500,
        offset: 500,
        blank: seed.is_multiple_of(2),
    };
    let mut presented = daemon.surface();
    let mut in_flight: VecDeque<i64> = VecDeque::new();
    // A partial answer on the last row asked for reads as all of them,
    // which no row can tell apart: the pane lands, and the answers still in
    // flight would read as unasked. That one surface goes unchecked, and the
    // scenario restarts from the daemon once it has served the rest.
    let (mut checked, mut exempt) = (0, 0);
    let mut trace = Vec::new();
    let mut now = Instant::now();
    for step in 0..400 {
        let fail = |trace: &[String]| {
            let recent = &trace[trace.len().saturating_sub(12)..];
            format!("seed {seed}, step {step}:\n{}", recent.join("\n"))
        };
        now += MS * rng.below(20) as u32;
        match rng.below(10) {
            0..4 => {
                let rows = (rng.below(201) as f32 - 100.) / 100.;
                let lines = scroll.wheel(&presented, "pane", rows, now);
                trace.push(format!("wheel {rows} sends {lines:?}"));
                if let Some(lines) = lines.filter(|lines| *lines != 0) {
                    in_flight.push_back(i64::from(lines));
                }
            }
            4..8 => {
                // A picture is found only while half the pane overlaps the
                // last, so at most two requests and half a pane a surface.
                let mut served = rng.below(3).min(in_flight.len() as u64) as usize;
                while in_flight.range(..served).sum::<i64>().abs() > i64::from(ROWS / 2) {
                    served -= 1;
                }
                let last = daemon.offset as i64 + in_flight.iter().sum::<i64>();
                let before = scroll.motion.as_ref().map(|motion| motion.target);
                for lines in in_flight.drain(..served) {
                    daemon.scroll(lines);
                }
                let ambiguous = !in_flight.is_empty() && daemon.offset as i64 == last;
                let output = rng.below(4) == 0;
                if output {
                    // Herdr keeps a scrolled-back view where it is.
                    daemon.offset += 1;
                }
                let next = daemon.surface();
                scroll.observe(&presented, &next);
                presented = next;
                trace.push(format!(
                    "serve {served} output {output}: at {}, {in_flight:?} left",
                    daemon.offset
                ));
                if ambiguous {
                    exempt += 1;
                    for lines in in_flight.drain(..) {
                        daemon.scroll(lines);
                    }
                    presented = daemon.surface();
                    scroll.clear();
                    trace.push(format!("ambiguous: restart at {}", daemon.offset));
                    continue;
                }
                // An answer never ends the motion or moves its target,
                // beyond what the output adds.
                if let Some(before) = before {
                    let target = scroll.motion.as_ref().map(|motion| motion.target);
                    let expected = before + f64::from(u8::from(output));
                    assert_eq!(target, Some(expected), "{}", fail(&trace));
                }
            }
            8 => {
                trace.push(format!("paint {:?}", scroll.slide(now).map(|s| s.offset)));
            }
            _ if in_flight.is_empty() => {
                let lines = rng.below(2) as i64 + 1;
                let lines = if rng.below(2) == 0 { lines } else { -lines };
                daemon.scroll(lines);
                let next = daemon.surface();
                scroll.observe(&presented, &next);
                presented = next;
                trace.push(format!("keyboard {lines}: at {}", daemon.offset));
                // A row the wheel never asked for lands where the daemon put it.
                if let Some(motion) = &scroll.motion {
                    assert_eq!(motion.target, motion.shown as f64, "{}", fail(&trace));
                }
            }
            _ => {}
        }
        // The motion heads where the daemon will be once it has served all.
        checked += 1;
        if let Some(motion) = &scroll.motion {
            let headed = daemon.offset as i64 + in_flight.iter().sum::<i64>();
            assert_eq!(motion.requested, headed, "{}", fail(&trace));
        }
    }
    (checked, exempt)
}

#[test]
fn the_wheel_keeps_in_step_with_a_daemon_that_answers_in_its_own_time() {
    let (checked, exempt) = (1..=500)
        .map(run)
        .fold((0, 0), |(c, e), (rc, re)| (c + rc, e + re));
    // About 2% of steps hit the ambiguity; most must stay checked.
    assert!(exempt * 20 < checked, "{exempt} of {checked} steps exempt");
}
