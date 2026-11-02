//! What `fl!` evaluates while compiling: a call naming a message the
//! English files lack, an argument the message does not take, or leaving
//! out one it does, stops the build at that call.

use super::MESSAGES;
use std::cmp::Ordering;

pub(crate) const fn check(id: &str, args: &[&str]) {
    let Some(index) = find(id) else {
        panic!("fl!: no message has this ID in locales/en-US");
    };
    let takes = MESSAGES[index].1;
    let mut i = 0;
    while i < args.len() {
        if !contains(takes, args[i]) {
            panic!("fl!: the message takes no argument by this name");
        }
        if contains(split(args, i), args[i]) {
            panic!("fl!: an argument is given twice");
        }
        i += 1;
    }
    let mut i = 0;
    while i < takes.len() {
        if !contains(args, takes[i]) {
            panic!("fl!: the message takes an argument this call leaves out");
        }
        i += 1;
    }
}

/// `MESSAGES` is sorted by ID.
const fn find(id: &str) -> Option<usize> {
    let (mut low, mut high) = (0, MESSAGES.len());
    while low < high {
        let middle = low + (high - low) / 2;
        match compare(MESSAGES[middle].0, id) {
            Ordering::Less => low = middle + 1,
            Ordering::Greater => high = middle,
            Ordering::Equal => return Some(middle),
        }
    }
    None
}

/// The arguments before `end`.
const fn split<'a>(args: &'a [&'a str], end: usize) -> &'a [&'a str] {
    args.split_at(end).0
}

const fn contains(names: &[&str], name: &str) -> bool {
    let mut i = 0;
    while i < names.len() {
        if matches!(compare(names[i], name), Ordering::Equal) {
            return true;
        }
        i += 1;
    }
    false
}

const fn compare(a: &str, b: &str) -> Ordering {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    let mut i = 0;
    while i < a.len() && i < b.len() {
        if a[i] < b[i] {
            return Ordering::Less;
        }
        if a[i] > b[i] {
            return Ordering::Greater;
        }
        i += 1;
    }
    if a.len() < b.len() {
        Ordering::Less
    } else if a.len() > b.len() {
        Ordering::Greater
    } else {
        Ordering::Equal
    }
}
