#![allow(clippy::unwrap_used)]
use super::*;

#[test]
fn backoff_doubles_up_to_its_limit() {
    let backoff = Timing::default().backoff;
    let seconds = |failures| backoff.delay(failures).as_secs();
    assert_eq!([0, 1, 2, 3, 4, 6, 7].map(seconds), [1, 1, 2, 4, 8, 32, 60]);
    assert_eq!(seconds(u32::MAX), 60);
}

#[test]
fn the_address_prints_its_port_alone() {
    let address = Address {
        port: NonZeroU16::new(51234).unwrap(),
        url: WebUrl::try_from("http://127.0.0.1:51234/?tkn=secret").unwrap(),
    };
    let printed = format!("{address:?}");
    assert!(printed.contains("51234"));
    assert!(!printed.contains("secret"), "{printed}");
}

#[cfg(unix)]
mod process {
    use super::*;
    use crate::code_server::Error;
    use std::sync::Mutex as StdMutex;

    const COMMIT: &str = "2a59476c9bfcb90b3ddc372c36762471b7dfad1c";

    /// A stand-in for VS Code's command line: it records its arguments and
    /// each start in `dir`, says it listens as `serve-web` does, then runs
    /// `body`. `$DIR` is `dir`.
    struct Fake {
        dir: tempfile::TempDir,
        program: PathBuf,
    }

    impl Fake {
        fn new(body: &str) -> Self {
            Self::quiet(&format!(
                "echo 'Web UI available at http://127.0.0.1/'\n{body}"
            ))
        }

        /// One that never says it listens, as when its port was taken.
        fn quiet(body: &str) -> Self {
            let dir = tempfile::tempdir().unwrap();
            let program = dir.path().join("code-tunnel");
            let script = format!(
                "#!/bin/sh\nDIR='{}'\nprintf '%s\\n' \"$@\" > \"$DIR/args\"\necho $$ >> \"$DIR/runs\"\n{body}\n",
                dir.path().display()
            );
            crate::test_executable::write(&program, script, 0o755).unwrap();
            Self { dir, program }
        }

        fn path(&self, name: &str) -> PathBuf {
            self.dir.path().join(name)
        }

        fn plan(&self, port: Option<NonZeroU16>) -> Plan {
            Plan {
                program: self.program.clone(),
                port,
                token_file: self.path("vscode-token"),
            }
        }

        fn read(&self, name: &str) -> String {
            std::fs::read_to_string(self.path(name)).unwrap_or_default()
        }

        fn runs(&self) -> usize {
            self.read("runs").lines().count()
        }

        /// A probe that answers once the fake has written `ready`.
        fn probe(&self) -> impl Fn(&WebUrl) -> crate::Result<Server> + Send + 'static {
            let ready = self.path("ready");
            move |_| {
                if ready.exists() {
                    Ok(Server::from_version(COMMIT).unwrap())
                } else {
                    Err(Error::NotServer { status: 200 }.into())
                }
            }
        }

        fn start(
            &self,
            port: Option<NonZeroU16>,
            probe: impl Fn(&WebUrl) -> crate::Result<Server> + Send + 'static,
        ) -> (Supervisor, Arc<StdMutex<Vec<NonZeroU16>>>) {
            let saved = Arc::new(StdMutex::new(Vec::new()));
            let recorded = saved.clone();
            let supervisor = Supervisor::start_with(
                self.plan(port),
                serve_web,
                probe,
                move |port| {
                    recorded.lock().unwrap().push(port);
                    Ok(())
                },
                fast(),
            )
            .unwrap();
            (supervisor, saved)
        }
    }

    fn fast() -> Timing {
        Timing {
            poll: Duration::from_millis(10),
            ready_timeout: Duration::from_secs(10),
            grace: Duration::from_secs(1),
            backoff: Backoff {
                first: Duration::from_millis(10),
                max: Duration::from_millis(40),
                healthy: Duration::from_secs(60),
            },
        }
    }

    /// Waits up to 10 seconds for `done`.
    fn until(what: &str, mut done: impl FnMut() -> bool) {
        let deadline = Instant::now() + Duration::from_secs(10);
        while !done() {
            assert!(Instant::now() < deadline, "timed out waiting for {what}");
            thread::sleep(Duration::from_millis(10));
        }
    }

    fn free() -> NonZeroU16 {
        let port = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
            .unwrap()
            .local_addr()
            .unwrap()
            .port();
        NonZeroU16::new(port).unwrap()
    }

    /// Whether `pid` has ended. One whose parent ended is reaped by the
    /// system, so it may stay a zombie for a moment.
    fn gone(pid: &str) -> bool {
        let output = Command::new("/bin/ps")
            .args(["-o", "stat=", "-p", pid.trim()])
            .output()
            .unwrap();
        let state = String::from_utf8(output.stdout).unwrap();
        state.trim().is_empty() || state.trim().starts_with('Z')
    }

    fn failure(supervisor: &Supervisor) -> Option<Arc<crate::Error>> {
        match supervisor.report().status {
            Status::Failed { error, .. } => Some(error),
            Status::Starting | Status::Ready => None,
        }
    }

    #[test]
    fn serve_web_starts_on_loopback_with_the_token_file_and_a_saved_port() {
        let fake = Fake::new("touch \"$DIR/ready\"\nexec sleep 60");
        let (supervisor, saved) = fake.start(None, fake.probe());
        until("VS Code to answer", || {
            matches!(supervisor.report().status, Status::Ready)
        });
        let report = supervisor.report();
        let address = report.address.clone().unwrap();
        // The port picked is saved once, so the next launch keeps it.
        assert_eq!(saved.lock().unwrap().as_slice(), [address.port]);
        let token_file = fake.path("vscode-token");
        let token = std::fs::read_to_string(&token_file).unwrap();
        assert_eq!(
            fake.read("args").lines().collect::<Vec<_>>(),
            [
                "serve-web",
                "--host",
                "127.0.0.1",
                "--port",
                &address.port.to_string(),
                "--connection-token-file",
                &token_file.display().to_string(),
                "--accept-server-license-terms",
            ]
        );
        assert_eq!(
            address.url.as_str(),
            format!("http://127.0.0.1:{}/?tkn={token}", address.port)
        );
        // What may be logged or shown never holds the token.
        assert!(!format!("{report:?}").contains(&token));
        assert!(!fake.read("args").contains(&token));
        supervisor.shutdown(Duration::from_secs(1));
    }

    #[test]
    fn a_saved_port_is_used_and_not_saved_again() {
        let fake = Fake::new("touch \"$DIR/ready\"\nexec sleep 60");
        let port = free();
        let (supervisor, saved) = fake.start(Some(port), fake.probe());
        until("VS Code to answer", || {
            matches!(supervisor.report().status, Status::Ready)
        });
        assert_eq!(supervisor.report().address.unwrap().port, port);
        assert!(fake.read("args").contains(&format!("\n{port}\n")));
        assert!(saved.lock().unwrap().is_empty());
        supervisor.shutdown(Duration::from_secs(1));
    }

    #[test]
    fn a_port_another_program_holds_is_named_and_nothing_starts() {
        let fake = Fake::new("exec sleep 60");
        let held = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let port = NonZeroU16::new(held.local_addr().unwrap().port()).unwrap();
        // Whatever holds it is never asked anything: each question carries
        // the token.
        let asked = Arc::new(StdMutex::new(0));
        let counted = asked.clone();
        let (supervisor, _) = fake.start(Some(port), move |_| {
            *counted.lock().unwrap() += 1;
            Ok(Server::from_version(COMMIT).unwrap())
        });

        until("the port check", || failure(&supervisor).is_some());
        let error = failure(&supervisor).unwrap();
        assert!(
            matches!(&*error, crate::Error::Code(Error::PortTaken { port: taken }) if *taken == port.get()),
            "{error:?}"
        );
        let message = error.to_string();
        assert!(
            message.contains(&format!("Port {port} on 127.0.0.1 is in use")),
            "{message}"
        );
        let token = std::fs::read_to_string(fake.path("vscode-token")).unwrap();
        assert!(!message.contains(&token));
        // It keeps checking, in case the other program quits, but never
        // starts VS Code on a port it does not hold.
        thread::sleep(Duration::from_millis(100));
        assert_eq!(fake.runs(), 0);
        assert_eq!(
            *asked.lock().unwrap(),
            0,
            "the token went to another program"
        );
        drop(supervisor);
    }

    /// A child that exits while it is being asked is not reported ready:
    /// whatever answered may be another program that took its port.
    #[test]
    fn an_answer_counts_only_if_the_child_outlived_it() {
        let fake = Fake::new("sleep 0.2\nexit 0");
        let ready = Arc::new(StdMutex::new(false));
        let (supervisor, _) = fake.start(None, |_| {
            // The child exits meanwhile; something answers anyway.
            thread::sleep(Duration::from_millis(500));
            Ok(Server::from_version(COMMIT).unwrap())
        });
        let seen = ready.clone();
        until("a failure", || {
            if matches!(supervisor.report().status, Status::Ready) {
                *seen.lock().unwrap() = true;
            }
            failure(&supervisor).is_some()
        });
        assert!(
            !*ready.lock().unwrap(),
            "reported ready on another's answer"
        );
        drop(supervisor);
    }

    /// A child whose port another program took first never binds it, and
    /// says nothing; the program that did must not be sent the token.
    #[test]
    fn a_child_that_never_says_it_listens_is_never_asked() {
        let fake = Fake::quiet("touch \"$DIR/ready\"\nexec sleep 60");
        let asked = Arc::new(StdMutex::new(0));
        let counted = asked.clone();
        let (supervisor, _) = fake.start(None, move |_| {
            *counted.lock().unwrap() += 1;
            Ok(Server::from_version(COMMIT).unwrap())
        });
        until("the child to start", || fake.runs() == 1);
        thread::sleep(Duration::from_millis(200));
        assert_eq!(*asked.lock().unwrap(), 0);
        assert!(matches!(supervisor.report().status, Status::Starting));
        supervisor.shutdown(Duration::from_millis(100));
    }

    #[test]
    fn shutdown_stops_the_child_and_what_it_started() {
        // Like serve-web, the child runs the server as a process of its own,
        // and ignores the request to stop, so it has to be killed.
        let fake = Fake::new(
            "trap '' TERM\nsleep 60 &\necho $! > \"$DIR/server\"\ntouch \"$DIR/ready\"\n\
             while :; do wait; done",
        );
        let (supervisor, _) = fake.start(None, fake.probe());
        until("VS Code to answer", || {
            matches!(supervisor.report().status, Status::Ready)
        });
        let (child, server) = (fake.read("runs"), fake.read("server"));
        assert!(!gone(&child) && !gone(&server));
        let started = Instant::now();
        supervisor.shutdown(Duration::from_millis(100));
        assert!(started.elapsed() < Duration::from_secs(2));
        until("the child to end", || gone(&child));
        until("its server to end", || gone(&server));
        // Nothing starts it again.
        thread::sleep(Duration::from_millis(100));
        assert_eq!(fake.runs(), 1);
    }

    /// Quitting while the worker starts the child waits for it, rather
    /// than finish before the child is there to stop.
    #[test]
    fn shutdown_during_a_start_stops_the_child_it_waited_for() {
        let fake = Fake::new("exec sleep 60");
        let (starting, started) = std::sync::mpsc::channel();
        let supervisor = Supervisor::start_with(
            fake.plan(None),
            move |plan, port| {
                let _ = starting.send(());
                thread::sleep(Duration::from_millis(300));
                serve_web(plan, port)
            },
            fake.probe(),
            |_| Ok(()),
            fast(),
        )
        .unwrap();
        started.recv_timeout(Duration::from_secs(10)).unwrap();
        let asked = Instant::now();
        supervisor.shutdown(Duration::from_secs(5));
        // It waited for the child to exist, then it, or the worker that saw
        // the stop, ended and reaped it.
        assert!(
            asked.elapsed() >= Duration::from_millis(200),
            "returned early"
        );
        until("the child to be reaped", || {
            supervisor.shared.lock().child.is_none()
        });
    }

    #[test]
    fn dropping_the_supervisor_stops_its_child_without_waiting() {
        let fake = Fake::new("sleep 60 &\necho $! > \"$DIR/server\"\ntouch \"$DIR/ready\"\nwait");
        let (supervisor, _) = fake.start(None, fake.probe());
        until("VS Code to answer", || {
            matches!(supervisor.report().status, Status::Ready)
        });
        let (child, server) = (fake.read("runs"), fake.read("server"));
        let started = Instant::now();
        drop(supervisor);
        assert!(started.elapsed() < Duration::from_millis(100));
        until("the child to end", || gone(&child));
        until("its server to end", || gone(&server));
    }

    #[test]
    fn a_child_that_stops_is_started_again_with_backoff() {
        let fake = Fake::new("exit 3");
        let (supervisor, _) = fake.start(None, fake.probe());
        until("three starts", || fake.runs() >= 3);
        // Between starts it says why it stopped.
        until("a failure", || failure(&supervisor).is_some());
        let error = failure(&supervisor).unwrap();
        assert_eq!(error.to_string(), "VS Code stopped (exit status: 3).");
        let Status::Failed { retry, .. } = supervisor.report().status else {
            panic!("not failed");
        };
        assert!(retry <= Instant::now() + fast().backoff.max);
        // The same port each time.
        let port = supervisor.report().address.unwrap().port;
        assert!(fake.read("args").contains(&format!("\n{port}\n")));
        drop(supervisor);
    }

    #[test]
    fn a_program_that_cannot_start_is_named() {
        let fake = Fake::new("");
        let plan = Plan {
            program: fake.path("missing"),
            ..fake.plan(None)
        };
        let supervisor =
            Supervisor::start_with(plan, serve_web, fake.probe(), |_| Ok(()), fast()).unwrap();
        until("the start to fail", || failure(&supervisor).is_some());
        let error = failure(&supervisor).unwrap();
        assert!(
            matches!(&*error, crate::Error::Code(Error::Spawn { .. })),
            "{error:?}"
        );
        assert!(error.to_string().contains("missing"));
        assert!(std::error::Error::source(&*error).is_some());
    }

    #[test]
    fn a_child_that_never_answers_is_stopped_and_started_again() {
        let fake = Fake::new("exec sleep 60");
        let supervisor = Supervisor::start_with(
            fake.plan(None),
            serve_web,
            fake.probe(),
            |_| Ok(()),
            Timing {
                ready_timeout: Duration::from_millis(100),
                ..fast()
            },
        )
        .unwrap();
        until("two starts", || fake.runs() >= 2);
        let first = fake.read("runs");
        let first = first.lines().next().unwrap();
        until("the first child to end", || gone(first));
        assert!(matches!(
            failure(&supervisor).as_deref(),
            Some(crate::Error::Code(Error::Silent(_))) | None
        ));
        supervisor.shutdown(Duration::from_millis(100));
    }
}
