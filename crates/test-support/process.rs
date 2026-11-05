use std::io;
#[cfg(target_os = "macos")]
use std::mem::size_of;

pub(crate) fn is_dead(pid: u32) -> io::Result<bool> {
    #[cfg(target_os = "linux")]
    {
        let stat = match std::fs::read_to_string(format!("/proc/{pid}/stat")) {
            Ok(stat) => stat,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(true),
            Err(error) => return Err(error),
        };
        let Some(state) = stat
            .rsplit_once(')')
            .and_then(|(_, fields)| fields.split_ascii_whitespace().next())
        else {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "malformed /proc process stat",
            ));
        };
        Ok(state == "Z")
    }

    #[cfg(target_os = "macos")]
    {
        let pid = libc::pid_t::try_from(pid)
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "process id out of range"))?;
        // libc has no safe libproc wrapper; zero is valid for this integer and byte-array struct.
        let mut info = unsafe { std::mem::zeroed::<libc::proc_bsdinfo>() };
        // proc_pidinfo writes at most the buffer size; read info only after a full result.
        let bytes = unsafe {
            libc::proc_pidinfo(
                pid,
                libc::PROC_PIDTBSDINFO,
                0,
                (&mut info as *mut libc::proc_bsdinfo).cast(),
                size_of::<libc::proc_bsdinfo>() as libc::c_int,
            )
        };
        if bytes == size_of::<libc::proc_bsdinfo>() as libc::c_int {
            return Ok(info.pbi_status == libc::SZOMB);
        }

        let error = io::Error::last_os_error();
        if error.raw_os_error() == Some(libc::ESRCH) {
            return Ok(true);
        }
        Err(error)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::is_dead;
    use std::process::{Child, Command};
    use std::thread;
    use std::time::{Duration, Instant};

    fn sleep() -> Child {
        Command::new("/bin/sleep").arg("60").spawn().unwrap()
    }

    #[test]
    fn reports_running_as_alive_and_zombie_and_reaped_as_dead() {
        let mut child = sleep();
        let pid = child.id();
        let running = is_dead(pid);

        child.kill().unwrap();
        assert!(!running.unwrap());
        let deadline = Instant::now() + Duration::from_secs(2);
        while !is_dead(pid).unwrap() {
            assert!(Instant::now() < deadline, "killed child did not exit");
            thread::sleep(Duration::from_millis(10));
        }

        child.wait().unwrap();
        assert!(is_dead(pid).unwrap());
    }
}
