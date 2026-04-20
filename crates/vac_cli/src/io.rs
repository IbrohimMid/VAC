use std::io;

pub async fn read_line_async() -> anyhow::Result<String> {
    tokio::task::spawn_blocking(|| {
        let mut s = String::new();
        io::stdin().read_line(&mut s)?;
        Ok(s)
    })
    .await?
}

pub async fn read_secret_async() -> anyhow::Result<String> {
    tokio::task::spawn_blocking(|| read_secret()).await?
}

fn read_secret() -> anyhow::Result<String> {
    #[cfg(unix)]
    {
        use std::os::unix::io::AsRawFd;
        if unsafe { libc_isatty(io::stdin().as_raw_fd()) } {
            return read_hidden();
        }
    }
    let mut s = String::new();
    io::stdin().read_line(&mut s)?;
    Ok(s.trim().to_string())
}

#[cfg(unix)]
struct TermiosGuard {
    fd: i32,
    original: libc_termios,
}

#[cfg(unix)]
impl Drop for TermiosGuard {
    fn drop(&mut self) {
        unsafe {
            libc_tcsetattr(self.fd, TCSANOW, &self.original);
        }
        println!();
    }
}

#[cfg(unix)]
fn read_hidden() -> anyhow::Result<String> {
    let stdin_fd = {
        use std::os::unix::io::AsRawFd;
        io::stdin().as_raw_fd()
    };

    let mut termios = unsafe {
        let mut t = std::mem::zeroed::<libc_termios>();
        if libc_tcgetattr(stdin_fd, &mut t) != 0 {
            let mut s = String::new();
            io::stdin().read_line(&mut s)?;
            return Ok(s.trim().to_string());
        }
        t
    };

    let _guard = TermiosGuard {
        fd: stdin_fd,
        original: termios,
    };

    termios.c_lflag &= !ECHO_FLAG;
    unsafe {
        libc_tcsetattr(stdin_fd, TCSANOW, &termios);
    }

    let mut s = String::new();
    let result = io::stdin().read_line(&mut s);
    result?;

    Ok(s.trim().to_string())
}

#[cfg(unix)]
#[repr(C)]
#[derive(Clone, Copy)]
struct libc_termios {
    c_iflag: u32,
    c_oflag: u32,
    c_cflag: u32,
    c_lflag: u32,
    c_line: u8,
    c_cc: [u8; 32],
    c_ispeed: u32,
    c_ospeed: u32,
}

#[cfg(unix)]
const ECHO_FLAG: u32 = 0x00000008; // ECHO
#[cfg(unix)]
const TCSANOW: i32 = 0;

#[cfg(unix)]
unsafe extern "C" {
    fn tcgetattr(fd: i32, termios: *mut libc_termios) -> i32;
    fn tcsetattr(fd: i32, optional_actions: i32, termios: *const libc_termios) -> i32;
    fn isatty(fd: i32) -> i32;
}

#[cfg(unix)]
unsafe fn libc_tcgetattr(fd: i32, t: *mut libc_termios) -> i32 {
    unsafe { tcgetattr(fd, t) }
}
#[cfg(unix)]
unsafe fn libc_tcsetattr(fd: i32, a: i32, t: *const libc_termios) -> i32 {
    unsafe { tcsetattr(fd, a, t) }
}
#[cfg(unix)]
unsafe fn libc_isatty(fd: i32) -> bool {
    unsafe { isatty(fd) != 0 }
}
