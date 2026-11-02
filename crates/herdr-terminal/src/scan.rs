//! A small pass over the byte stream before vte sees it. It does two things
//! vte cannot: it tracks Win32 input mode (DECSET/DECRST `?9001`), which
//! alacritty ignores, and it bounds OSC strings, which vte buffers without a
//! limit when built with `std`. Its state carries across feeds, so a sequence
//! split anywhere gives the same result as one fed whole.

use crate::MAX_OSC_BYTES;

const ESC: u8 = 0x1b;
const BEL: u8 = 0x07;
const CAN: u8 = 0x18;
const SUB: u8 = 0x1a;
const WIN32_INPUT_MODE: u32 = 9001;
/// Parameters past this are as invalid to vte (u16) as to the mode check.
const PARAM_CAP: u32 = 100_000;

#[derive(Debug, Default)]
pub(crate) struct Scanner {
    state: State,
    win32_input: bool,
    /// The held OSC string, from its `]` on; vte has seen only its ESC.
    osc: Vec<u8>,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
enum State {
    #[default]
    Ground,
    Escape,
    Csi(Csi),
    /// Holding an OSC string until its terminator.
    Osc,
    /// Holding an OSC string whose last byte was ESC.
    OscEscape,
    /// Dropping an OSC string longer than [`MAX_OSC_BYTES`].
    Overflow,
    /// Dropping an oversized OSC string whose last byte was ESC.
    OverflowEscape,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
struct Csi {
    started: bool,
    private: bool,
    invalid: bool,
    param: u32,
    matched: bool,
}

impl Csi {
    fn end_param(&mut self) {
        self.matched |= self.param == WIN32_INPUT_MODE;
        self.param = 0;
    }
}

impl Scanner {
    pub(crate) fn win32_input(&self) -> bool {
        self.win32_input
    }

    /// Passes `bytes` on to `forward` in order, minus held or dropped OSC
    /// strings, which it passes on whole once they end.
    pub(crate) fn feed(&mut self, bytes: &[u8], mut forward: impl FnMut(&[u8])) {
        let mut run = 0;
        let mut index = 0;
        while index < bytes.len() {
            let byte = bytes[index];
            match self.state {
                State::Ground => {
                    if byte == ESC {
                        self.state = State::Escape;
                    }
                }
                State::Escape => self.escape(byte, bytes, &mut run, index, &mut forward),
                State::Csi(mut csi) => self.state = csi_byte(&mut csi, byte, &mut self.win32_input),
                State::Osc => match byte {
                    BEL | CAN | SUB => {
                        self.osc.push(byte);
                        forward(&self.osc);
                        self.end_osc(&mut run, index + 1);
                    }
                    ESC => self.state = State::OscEscape,
                    _ if self.osc.len() >= MAX_OSC_BYTES => {
                        // vte holds the ESC that began this string; `\`
                        // makes it a harmless ST.
                        forward(b"\\");
                        self.osc = Vec::new();
                        self.state = State::Overflow;
                    }
                    _ => self.osc.push(byte),
                },
                State::OscEscape => {
                    self.osc.push(ESC);
                    if byte == b'\\' {
                        self.osc.push(byte);
                        forward(&self.osc);
                        self.end_osc(&mut run, index + 1);
                    } else {
                        // ESC ends the string and begins another sequence
                        // with this byte, which is read again as such.
                        forward(&self.osc);
                        self.end_osc(&mut run, index);
                        self.state = State::Escape;
                        continue;
                    }
                }
                State::Overflow => match byte {
                    ESC => self.state = State::OverflowEscape,
                    BEL | CAN | SUB => self.end_osc(&mut run, index + 1),
                    _ => {}
                },
                State::OverflowEscape => {
                    if byte == b'\\' {
                        self.end_osc(&mut run, index + 1);
                    } else {
                        forward(&[ESC]);
                        self.end_osc(&mut run, index);
                        self.state = State::Escape;
                        continue;
                    }
                }
            }
            index += 1;
        }
        if !self.holding() && run < bytes.len() {
            forward(&bytes[run..]);
        }
    }

    fn escape(
        &mut self,
        byte: u8,
        bytes: &[u8],
        run: &mut usize,
        index: usize,
        forward: &mut impl FnMut(&[u8]),
    ) {
        self.state = match byte {
            b'[' => State::Csi(Csi::default()),
            b']' => {
                if *run < index {
                    forward(&bytes[*run..index]);
                }
                *run = index + 1;
                self.osc.clear();
                self.osc.push(byte);
                State::Osc
            }
            b'c' => {
                // RIS resets every mode.
                self.win32_input = false;
                State::Ground
            }
            ESC => State::Escape,
            // vte runs C0 controls inside an escape without leaving it.
            0x00..=0x17 | 0x19 | 0x1c..=0x1f | 0x7f => State::Escape,
            _ => State::Ground,
        };
    }

    fn end_osc(&mut self, run: &mut usize, next: usize) {
        if self.osc.capacity() > MAX_OSC_BYTES {
            self.osc = Vec::new();
        } else {
            self.osc.clear();
        }
        *run = next;
        self.state = State::Ground;
    }

    fn holding(&self) -> bool {
        matches!(
            self.state,
            State::Osc | State::OscEscape | State::Overflow | State::OverflowEscape
        )
    }
}

fn csi_byte(csi: &mut Csi, byte: u8, win32_input: &mut bool) -> State {
    match byte {
        ESC => State::Escape,
        CAN | SUB => State::Ground,
        b'0'..=b'9' => {
            csi.started = true;
            csi.param = (csi.param * 10 + u32::from(byte - b'0')).min(PARAM_CAP);
            State::Csi(*csi)
        }
        b';' | b':' => {
            csi.started = true;
            csi.end_param();
            State::Csi(*csi)
        }
        b'<'..=b'?' => {
            if !csi.started && !csi.private && byte == b'?' {
                csi.private = true;
            } else {
                csi.invalid = true;
            }
            csi.started = true;
            State::Csi(*csi)
        }
        0x20..=0x2f => {
            csi.invalid = true;
            State::Csi(*csi)
        }
        0x40..=0x7e => {
            csi.end_param();
            if csi.private && !csi.invalid && csi.matched {
                match byte {
                    b'h' => *win32_input = true,
                    b'l' => *win32_input = false,
                    _ => {}
                }
            }
            State::Ground
        }
        _ => State::Csi(*csi),
    }
}
