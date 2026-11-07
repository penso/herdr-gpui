use super::*;
use crate::pane_menu::{cpu_text, memory_text};

fn row(number: u16, pid: u32, cpu: Option<f32>, memory: u64, system: bool) -> Listener {
    Listener {
        cpu,
        memory,
        system,
        ..listener(number, pid, 10)
    }
}

fn ports(rows: &[&Listener]) -> Vec<u16> {
    rows.iter().map(|row| row.number).collect()
}

#[test]
fn the_users_servers_come_before_the_systems_in_every_order() {
    let listing = vec![
        row(5000, 100, Some(1.), 10 << 20, true),
        row(8080, 4343, Some(40.), 300 << 20, false),
        row(3000, 4242, None, 900 << 20, false),
        row(7000, 101, Some(90.), 2 << 30, true),
        row(5173, 4444, Some(2.5), 80 << 20, false),
    ];
    assert_eq!(
        ports(&order(&listing, Sort::Port)),
        [3000, 5173, 8080, 5000, 7000]
    );
    // Not yet measured counts as idle.
    assert_eq!(
        ports(&order(&listing, Sort::Cpu)),
        [8080, 5173, 3000, 7000, 5000]
    );
    assert_eq!(
        ports(&order(&listing, Sort::Memory)),
        [3000, 8080, 5173, 7000, 5000]
    );
}

#[test]
fn ties_keep_port_then_pid_order() {
    let listing = vec![
        row(8080, 4344, Some(5.), 1, false),
        row(8080, 4343, Some(5.), 1, false),
        row(3000, 4242, Some(5.), 1, false),
    ];
    let pids: Vec<u32> = order(&listing, Sort::Cpu)
        .iter()
        .map(|row| row.identity.pid)
        .collect();
    assert_eq!(pids, [4242, 4343, 4344]);
}

#[test]
fn usage_reads_as_the_process_list_shows_it() {
    assert_eq!(cpu_text(None), "–");
    assert_eq!(cpu_text(Some(2.54)), "2.5%");
    assert_eq!(cpu_text(Some(140.)), "140%");
    assert_eq!(memory_text(80 << 20), "\u{2068}80\u{2069} MB");
    assert_eq!(memory_text(2 << 30), "\u{2068}2.0\u{2069} GB");
}
