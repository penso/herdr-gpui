use std::io;
#[cfg(target_os = "macos")]
use std::mem::size_of;

pub(crate) fn is_dead(pid: u32) -> io::Result<bool> {
    #[cfg(target_os = "linux")]
    {
        linux_stat_dead(std::fs::read_to_string(format!("/proc/{pid}/stat")))
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

#[cfg(target_os = "linux")]
fn linux_stat_dead(stat: io::Result<String>) -> io::Result<bool> {
    let stat = match stat {
        Ok(stat) => stat,
        // A process reaped before open gives ENOENT; one reaped between
        // open and read can give ESRCH from procfs instead.
        Err(error)
            if error.kind() == io::ErrorKind::NotFound
                || error.raw_os_error() == Some(libc::ESRCH) =>
        {
            return Ok(true);
        }
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

#[cfg(test)]
#[allow(clippy::unwrap_used)]
#[path = "process/tests.rs"]
mod tests;
