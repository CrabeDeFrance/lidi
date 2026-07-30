use std::{io, mem, net, path, pin, ptr};

/// Removes a stale Unix domain socket file left over from a previous run so
/// that a listener can re-bind the path.
///
/// # Errors
///
/// Will return `Err` if `path` exists but is not a socket (refuses to delete
/// unrelated user files), or if removing the stale socket fails.
pub fn remove_stale_unix_socket(path: &path::Path) -> Result<(), io::Error> {
    use std::os::unix::fs::FileTypeExt;

    let metadata = match std::fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(e),
    };

    if !metadata.file_type().is_socket() {
        return Err(io::Error::other(format!(
            "'{}' already exists and is not a socket, refusing to delete it",
            path.display()
        )));
    }

    std::fs::remove_file(path)
}

#[cfg(not(target_os = "freebsd"))]
type TypeConstMmsgBatchSize = u32;
#[cfg(target_os = "freebsd")]
type TypeConstMmsgBatchSize = usize;

pub const MAX_MMSG_BATCH_SIZE: TypeConstMmsgBatchSize = 1024;

pub fn convert_address(
    dest: net::SocketAddr,
) -> Result<(pin::Pin<Box<libc::sockaddr>>, usize), io::Error> {
    let (dest, dest_len) = match dest {
        net::SocketAddr::V4(addr4) => {
            let addr = libc::sockaddr_in {
                sin_family: libc::sa_family_t::try_from(libc::AF_INET).map_err(|e| {
                    io::Error::new(io::ErrorKind::InvalidData, format!("libc::AF_INET: {e}"))
                })?,
                sin_addr: libc::in_addr {
                    s_addr: u32::from_le_bytes(addr4.ip().octets()),
                },
                sin_port: addr4.port().to_be(),
                sin_zero: [0; 8],
                #[cfg(target_os = "freebsd")]
                sin_len: 0,
            };
            let addr = Box::new(addr);
            (
                unsafe {
                    mem::transmute::<
                        std::boxed::Box<libc::sockaddr_in>,
                        std::boxed::Box<libc::sockaddr>,
                    >(addr)
                },
                mem::size_of::<libc::sockaddr_in>(),
            )
        }
        net::SocketAddr::V6(addr6) => {
            let addr = libc::sockaddr_in6 {
                sin6_family: libc::sa_family_t::try_from(libc::AF_INET6).map_err(|e| {
                    io::Error::new(io::ErrorKind::InvalidData, format!("libc::AF_INET6: {e}"))
                })?,
                sin6_port: addr6.port().to_be(),
                sin6_flowinfo: addr6.flowinfo(),
                sin6_addr: libc::in6_addr {
                    s6_addr: addr6.ip().octets(),
                },
                sin6_scope_id: addr6.scope_id(),
                #[cfg(target_os = "freebsd")]
                sin6_len: 0,
            };
            let addr = Box::new(addr);
            (
                unsafe {
                    mem::transmute::<
                        std::boxed::Box<libc::sockaddr_in6>,
                        std::boxed::Box<libc::sockaddr>,
                    >(addr)
                },
                mem::size_of::<libc::sockaddr_in6>(),
            )
        }
    };

    Ok((pin::Pin::new(dest), dest_len))
}

pub fn getsockopt_buffer_size(fd: i32, option_name: i32) -> Result<i32, io::Error> {
    let mut sz = 0i32;
    let mut len = libc::socklen_t::try_from(mem::size_of::<libc::c_int>())
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, format!("len: {e}")))?;
    let res = unsafe {
        libc::getsockopt(
            fd,
            libc::SOL_SOCKET,
            option_name,
            ptr::addr_of_mut!(sz).cast::<libc::c_void>(),
            &raw mut len,
        )
    };
    if res == 0 {
        Ok(sz)
    } else {
        #[cfg(not(target_os = "freebsd"))]
        let errno = unsafe { *libc::__errno_location() };
        #[cfg(target_os = "freebsd")]
        let errno = unsafe { *libc::__error() };
        Err(io::Error::other(format!(
            "libc::getsockopt returned {res}, errno == {errno}",
        )))
    }
}

pub fn setsockopt_buffer_size(fd: i32, size: i32, option_name: i32) -> Result<(), io::Error> {
    let len = libc::socklen_t::try_from(mem::size_of::<libc::c_int>())
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, format!("len: {e}")))?;

    let res = unsafe {
        libc::setsockopt(
            fd,
            libc::SOL_SOCKET,
            option_name,
            ptr::addr_of!(size).cast::<libc::c_void>(),
            len,
        )
    };

    if res == 0 {
        Ok(())
    } else {
        #[cfg(not(target_os = "freebsd"))]
        let errno = unsafe { *libc::__errno_location() };
        #[cfg(target_os = "freebsd")]
        let errno = unsafe { *libc::__error() };
        Err(io::Error::other(format!(
            "libc::setsockopt returned {res}, errno == {errno}",
        )))
    }
}
