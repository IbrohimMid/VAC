use std::io;

#[cfg(unix)]
pub fn apply_rlimit_as(limit_bytes: u64) -> io::Result<()> {
    unsafe {
        let rlim = libc::rlimit {
            rlim_cur: limit_bytes as libc::rlim_t,
            rlim_max: limit_bytes as libc::rlim_t,
        };
        if libc::setrlimit(libc::RLIMIT_AS, &rlim) != 0 {
            return Err(io::Error::last_os_error());
        }
    }
    Ok(())
}

#[cfg(unix)]
pub fn apply_rlimit_fsize(limit_bytes: u64) -> io::Result<()> {
    unsafe {
        let rlim = libc::rlimit {
            rlim_cur: limit_bytes as libc::rlim_t,
            rlim_max: limit_bytes as libc::rlim_t,
        };
        if libc::setrlimit(libc::RLIMIT_FSIZE, &rlim) != 0 {
            return Err(io::Error::last_os_error());
        }
    }
    Ok(())
}

#[cfg(not(unix))]
pub fn apply_rlimit_as(_limit_bytes: u64) -> io::Result<()> {
    Ok(())
}

#[cfg(not(unix))]
pub fn apply_rlimit_fsize(_limit_bytes: u64) -> io::Result<()> {
    Ok(())
}
