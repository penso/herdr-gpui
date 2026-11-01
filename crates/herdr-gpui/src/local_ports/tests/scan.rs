use super::*;
use crate::local_ports::scan::{MAX_SOCKETS, Socket, parse};
use std::net::{IpAddr, Ipv4Addr};

fn socket(pid: u32, number: u16, bind: Bind, process: &str) -> Socket {
    Socket {
        pid,
        number,
        bind,
        process: process.into(),
    }
}

#[test]
fn lsof_field_output_names_each_process_and_its_sockets() {
    let text = "lsof\np4242\ncnode\nf20\nn127.0.0.1:3000\nf21\nn[::1]:3000\np4343\ncpython3\nf3\nn*:8080\nf4\nn192.168.1.20:9000\n";
    assert_eq!(
        parse(text).unwrap(),
        vec![
            socket(4242, 3000, Bind::Loopback, "node"),
            socket(4343, 8080, Bind::Any, "python3"),
            socket(
                4343,
                9000,
                Bind::Address(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 20))),
                "python3"
            ),
        ]
    );
}

#[test]
fn ss_output_names_every_process_holding_a_socket() {
    let text = "ss\n\
State  Recv-Q Send-Q Local Address:Port Peer Address:Port Process\n\
LISTEN 0      511    127.0.0.1:5173     0.0.0.0:*         users:((\"node\",pid=4242,fd=20))\n\
LISTEN 0      4096   [::]:8080          [::]:*            users:((\"python3\",pid=4343,fd=3),(\"python3\",pid=4344,fd=3))\n\
LISTEN 0      4096   0.0.0.0:8080       0.0.0.0:*         users:((\"python3\",pid=4343,fd=4))\n\
LISTEN 0      128    0.0.0.0:22         0.0.0.0:*\n\
LISTEN 0      4096   127.0.0.53%lo:53   0.0.0.0:*\n";
    assert_eq!(
        parse(text).unwrap(),
        vec![
            socket(4242, 5173, Bind::Loopback, "node"),
            socket(4343, 8080, Bind::Any, "python3"),
            socket(4344, 8080, Bind::Any, "python3"),
        ]
    );
}

#[test]
fn one_process_on_ipv4_and_ipv6_is_one_row_at_its_wider_bind() {
    let text = "lsof\np4242\ncnode\nn127.0.0.1:3000\nn*:3000\n";
    assert_eq!(
        parse(text).unwrap(),
        vec![socket(4242, 3000, Bind::Any, "node")]
    );
}

#[test]
fn malformed_and_untrusted_lines_are_dropped() {
    let text = "lsof\nn*:1\npnot-a-pid\ncx\nn*:2\np0\nn*:3\np4242\nc\u{1b}[31mnode\u{7}\nn*:0\nn*:70000\nnhost:3000\nn*:3001\n\nq\n";
    assert_eq!(parse(text).unwrap(), vec![socket(4242, 3001, Bind::Any, "[31mnode")]);
    let ss = "ss\nLISTEN 0 1 127.0.0.1:3000 0.0.0.0:* users:((\"node\",pid=x,fd=1))\nLISTEN\nESTAB 0 0 127.0.0.1:3000 127.0.0.1:5000 users:((\"node\",pid=4242,fd=9))\n";
    assert_eq!(parse(ss).unwrap(), Vec::new());
}

#[test]
fn a_computer_without_ss_or_lsof_says_so() {
    assert_eq!(parse("none\n"), None);
    assert_eq!(parse(""), None);
    assert_eq!(parse("netstat\n"), None);
}

#[test]
fn an_answer_is_read_only_up_to_its_bound() {
    let mut text = String::from("lsof\np4242\ncnode\n");
    for number in 1..=(MAX_SOCKETS as u32 + 50) {
        text.push_str(&format!("n*:{number}\n"));
    }
    assert_eq!(parse(&text).unwrap().len(), MAX_SOCKETS);
    let mut text = String::from("ss\n");
    for number in 1..=(MAX_SOCKETS as u32 + 50) {
        text.push_str(&format!(
            "LISTEN 0 1 *:{number} *:* users:((\"node\",pid=4242,fd=1))\n"
        ));
    }
    assert_eq!(parse(&text).unwrap().len(), MAX_SOCKETS);
}

#[test]
fn this_computer_lists_a_socket_this_process_holds() {
    let socket = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let number = socket.local_addr().unwrap().port();
    let mut output = Vec::new();
    herdr_client::run_script(
        herdr_client::ScriptHost::Local,
        crate::local_ports::scan::SCRIPT,
        std::io::empty(),
        &mut output,
        herdr_client::ScriptLimits {
            output: 512 * 1024,
            idle: Duration::from_secs(15),
        },
        &AtomicBool::new(false),
    )
    .unwrap();
    let Some(sockets) = parse(&String::from_utf8_lossy(&output)) else {
        // A sandbox without ss or lsof cannot show it.
        return;
    };
    assert!(
        sockets
            .iter()
            .any(|socket| socket.number == number && socket.pid == std::process::id()),
        "{sockets:?}"
    );
}
