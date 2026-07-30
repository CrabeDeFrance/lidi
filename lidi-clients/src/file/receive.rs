use crate::file::{self, EntryType};
#[cfg(feature = "hash")]
use crate::hash;
#[cfg(feature = "tls")]
use crate::tls;
#[cfg(feature = "tcp")]
use std::net;
#[cfg(feature = "unix")]
use std::os::unix;
use std::{
    fs,
    io::{self, Read, Write},
    os::unix::fs::PermissionsExt,
    path, thread,
};

struct CompletedTransfer {
    size: u64,
    entry_type: file::EntryType,
    path: path::PathBuf,
}

/// # Errors
///
/// Will return `Err` if `output_dir` is not a directory.
pub fn receive_files(
    config: &file::Config<crate::DiodeReceive>,
    output_dir: &path::Path,
) -> Result<(), file::Error> {
    if !output_dir.is_dir() {
        return Err(file::Error::Other(String::from(
            "output_directory is not a directory",
        )));
    }

    thread::scope(|scope| -> Result<(), file::Error> {
        #[cfg(feature = "tcp")]
        if let Some(from_tcp) = &config.diode.from_tcp {
            let server = net::TcpListener::bind(from_tcp)?;
            thread::Builder::new().spawn_scoped(scope, move || {
                receive_tcp_loop(config, output_dir, scope, &server)
            })?;
        }

        #[cfg(feature = "tls")]
        if let Some(from_tls) = &config.diode.from_tls {
            let server = tls::TcpListener::bind(&config.tls, from_tls)?;
            thread::Builder::new().spawn_scoped(scope, move || {
                receive_tls_loop(config, output_dir, scope, &server)
            })?;
        }

        #[cfg(feature = "unix")]
        if let Some(from_unix) = &config.diode.from_unix {
            crate::remove_stale_unix_socket(from_unix).map_err(|e| {
                file::Error::Other(format!(
                    "Unix socket path '{}' already exists and cannot be deleted: {e}",
                    from_unix.display()
                ))
            })?;

            let server = unix::net::UnixListener::bind(from_unix)?;
            thread::Builder::new().spawn_scoped(scope, move || {
                receive_unix_loop(config, output_dir, scope, &server)
            })?;
        }

        Ok(())
    })
}

#[cfg(feature = "tcp")]
fn receive_tcp_loop<'a>(
    config: &'a file::Config<crate::DiodeReceive>,
    output_dir: &'a path::Path,
    scope: &'a thread::Scope<'a, '_>,
    server: &net::TcpListener,
) -> Result<(), file::Error> {
    let mut count = 0;

    loop {
        if config.max_files != 0 && count >= config.max_files {
            return Ok(());
        }
        count += 1;
        let (client, client_addr) = server.accept()?;
        log::debug!("new TCP client ({client_addr}) connected");
        scope.spawn(|| match receive_message(config, client, output_dir) {
            Ok(transfer) => log::info!(
                "{} \"{}\" received, {} bytes received",
                transfer.entry_type,
                transfer.path.display(),
                transfer.size
            ),
            Err(e) => log::error!("failed to receive file or directory: {e}"),
        });
    }
}

#[cfg(feature = "tls")]
fn receive_tls_loop<'a>(
    config: &'a file::Config<crate::DiodeReceive>,
    output_dir: &'a path::Path,
    scope: &'a thread::Scope<'a, '_>,
    server: &tls::TcpListener,
) -> Result<(), file::Error> {
    let mut count = 0;

    loop {
        if config.max_files != 0 && count >= config.max_files {
            return Ok(());
        }
        count += 1;
        let (client, client_addr) = server.accept()??;
        log::info!("new TLS client ({client_addr}) connected");
        scope.spawn(|| match receive_message(config, client, output_dir) {
            Ok(transfer) => log::info!(
                "{} \"{}\" received, {} bytes received",
                transfer.entry_type,
                transfer.path.display(),
                transfer.size
            ),
            Err(e) => log::error!("failed to receive file or directory: {e}"),
        });
    }
}

#[cfg(feature = "unix")]
fn receive_unix_loop<'a>(
    config: &'a file::Config<crate::DiodeReceive>,
    output_dir: &'a path::Path,
    scope: &'a thread::Scope<'a, '_>,
    server: &unix::net::UnixListener,
) -> Result<(), file::Error> {
    let mut count = 0;

    loop {
        if config.max_files != 0 && count >= config.max_files {
            return Ok(());
        }
        count += 1;
        let (client, client_addr) = server.accept()?;
        log::info!(
            "new Unix client ({}) connected",
            client_addr
                .as_pathname()
                .map_or_else(|| String::from("unknown"), |p| p.display().to_string())
        );
        scope.spawn(|| match receive_message(config, client, output_dir) {
            Ok(transfer) => log::info!(
                "{} \"{}\" received, {} bytes received",
                transfer.entry_type,
                transfer.path.display(),
                transfer.size
            ),
            Err(e) => log::error!("failed to receive file or directory: {e}"),
        });
    }
}

fn receive_message<D>(
    config: &file::Config<crate::DiodeReceive>,
    mut diode: D,
    output_dir: &path::Path,
) -> Result<CompletedTransfer, file::Error>
where
    D: Read,
{
    match file::protocol::Message::deserialize_from(&mut diode)? {
        file::protocol::Message::StartDirTransfer(info) => {
            let relative_path = path::PathBuf::from_iter(&info.path);
            let tmp_path = get_writing_path(config, output_dir, &relative_path)?;
            receive_messages_in_dir(config, &mut diode, &tmp_path)
                .and_then(|size| {
                    Ok(CompletedTransfer {
                        size,
                        entry_type: EntryType::Directory,
                        path: move_to_final_path(config, &tmp_path, output_dir, &relative_path)?,
                    })
                })
                .inspect_err(|_| {
                    if let Err(remove_err) = fs::remove_dir_all(&tmp_path)
                        && remove_err.kind() != io::ErrorKind::NotFound
                    {
                        log::warn!(
                            "failed to remove incomplete directory \"{}\": {remove_err}",
                            tmp_path.display()
                        );
                    }
                })
        }
        file::protocol::Message::FileEntry(header) => {
            let relative_path = path::PathBuf::from_iter(&header.info.path);
            let tmp_path = get_writing_path(config, output_dir, &relative_path)?;
            receive_file(config, &mut diode, &tmp_path, &header)
                .and_then(|size| {
                    Ok(CompletedTransfer {
                        size: size as u64,
                        entry_type: EntryType::File,
                        path: move_to_final_path(config, &tmp_path, output_dir, &relative_path)?,
                    })
                })
                .inspect_err(|_| {
                    if let Err(remove_err) = fs::remove_file(&tmp_path)
                        && remove_err.kind() != io::ErrorKind::NotFound
                    {
                        log::warn!(
                            "failed to remove incomplete file \"{}\": {remove_err}",
                            tmp_path.display()
                        );
                    }
                })
        }
        file::protocol::Message::EndDirTransfer => Err(file::Error::Other(
            "invalid \"End directory transfer\" message: no directory transfer in progress"
                .to_owned(),
        )),
        file::protocol::Message::DirEntry(_) => Err(file::Error::Other(
            "invalid \"Dir entry\" message: no directory transfer in progress".to_owned(),
        )),
    }
}

fn get_writing_path(
    config: &file::Config<crate::DiodeReceive>,
    output_dir: &path::Path,
    relative_path: &path::Path,
) -> Result<path::PathBuf, file::Error> {
    // Create in the tmp dir if configured, else directly in the output dir
    let base_dir = config
        .tmp_dir
        .as_ref()
        .map_or(output_dir, |tmp_dir| tmp_dir);
    let tmp_path = base_dir.join(relative_path);
    // We want to overwrite files in the temp dir even if config.overwrite == false
    let overwrite = config.overwrite || config.tmp_dir.is_some();
    check_overwrite(&tmp_path, overwrite)?;
    Ok(tmp_path)
}

fn move_to_final_path(
    config: &file::Config<crate::DiodeReceive>,
    tmp_path: &path::Path,
    output_dir: &path::Path,
    relative_path: &path::Path,
) -> Result<path::PathBuf, file::Error> {
    // move from tmp dir to output dir if configured as so
    if config.tmp_dir.is_some() {
        let final_dest = output_dir.join(relative_path);
        check_overwrite(&final_dest, config.overwrite)?;
        fs::rename(tmp_path, &final_dest)?;
        return Ok(final_dest);
    }
    Ok(tmp_path.to_owned())
}

fn check_overwrite(path: &path::Path, overwrite: bool) -> Result<(), file::Error> {
    let Ok(metadata) = fs::metadata(path) else {
        // dest does not exist -> create parent dir if necessary
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        return Ok(());
    };

    // dest exists -> error or delete depending on the configuration
    if overwrite {
        log::info!("overwriting \"{}\"", path.display());
        if metadata.is_dir() {
            fs::remove_dir_all(path)?;
        } else {
            fs::remove_file(path)?;
        }
        Ok(())
    } else {
        Err(file::Error::Other(format!(
            "\"{}\" already exists",
            path.display()
        )))
    }
}

fn receive_messages_in_dir<D>(
    config: &file::Config<crate::DiodeReceive>,
    mut diode: D,
    dir_path: &path::Path,
) -> Result<u64, file::Error>
where
    D: Read,
{
    log::info!("receiving directory \"{}\"", dir_path.display());
    fs::create_dir(dir_path)?;

    let mut total_size = 0;
    loop {
        match file::protocol::Message::deserialize_from(&mut diode)? {
            file::protocol::Message::StartDirTransfer(_) => {
                return Err(file::Error::Other(
                    "invalid \"Start directory transfer\" message: a directory transfer is already in progress"
                .to_owned(),
                ));
            }
            file::protocol::Message::EndDirTransfer => break,
            file::protocol::Message::FileEntry(header) => {
                let file_path = dir_path.join(path::PathBuf::from_iter(&header.info.path));
                total_size += receive_file(config, &mut diode, &file_path, &header)? as u64;
            }
            file::protocol::Message::DirEntry(info) => {
                let rel_path = path::PathBuf::from_iter(&info.path);
                log::info!("receiving subdirectory \"{}\"", rel_path.display());
                let subdir = dir_path.join(rel_path);
                fs::create_dir(&subdir)?;
                fs::set_permissions(&subdir, fs::Permissions::from_mode(info.mode))?;
            }
        }
    }
    Ok(total_size)
}

fn receive_file<D>(
    config: &file::Config<crate::DiodeReceive>,
    diode: &mut D,
    file_path: &path::Path,
    header: &file::protocol::FileHeader,
) -> Result<usize, file::Error>
where
    D: Read,
{
    log::info!(
        "receiving file \"{}\" ({} bytes)",
        file_path.display(),
        header.file_length
    );

    let mut file = fs::OpenOptions::new()
        .read(false)
        .write(true)
        .create(true)
        .truncate(true)
        .open(file_path)?;

    log::debug!("setting mode to {}", header.info.mode);
    file.set_permissions(fs::Permissions::from_mode(header.info.mode))?;

    let mut buffer = vec![0; config.buffer_size];
    let mut cursor = 0;
    let mut remaining = usize::try_from(header.file_length)?;

    #[cfg(feature = "hash")]
    let mut hasher = if config.hash {
        Some(hash::StreamHasher::default())
    } else {
        None
    };

    loop {
        let end = if remaining >= (config.buffer_size - cursor) {
            config.buffer_size
        } else {
            cursor + remaining
        };
        match diode.read(&mut buffer[cursor..end])? {
            0 => {
                if 0 < cursor {
                    #[cfg(feature = "hash")]
                    if let Some(hasher) = hasher.as_mut() {
                        hasher.update(&buffer[..cursor]);
                    }
                    file.write_all(&buffer[..cursor])?;
                }

                file.flush()?;

                let received = usize::try_from(header.file_length)? - remaining;

                if remaining != 0 {
                    log::debug!("expected file size = {}", header.file_length);
                    log::debug!("received file size = {received}");
                    return Err(file::Error::Diode(file::protocol::Error::InvalidFileSize(
                        usize::try_from(header.file_length)?,
                        received,
                    )));
                }

                #[allow(unused_variables)]
                let footer = file::protocol::Footer::deserialize_from(diode)?;
                #[cfg(feature = "hash")]
                if let Some(hasher) = hasher.as_mut() {
                    let hash = hasher.finalize();
                    log::debug!("expected hash = {}", footer.hash);
                    log::debug!("computed hash = {hash}");
                    if footer.hash != hash {
                        return Err(file::Error::Diode(file::protocol::Error::InvalidHash(
                            hash,
                            footer.hash,
                        )));
                    }
                }

                return Ok(received);
            }
            nread => {
                remaining -= nread;
                if (cursor + nread) < config.buffer_size {
                    cursor += nread;
                    continue;
                }
                #[cfg(feature = "hash")]
                if let Some(hasher) = hasher.as_mut() {
                    hasher.update(&buffer);
                }
                file.write_all(&buffer)?;
                cursor = 0;
            }
        }
    }
}
