//! Where the daemon listens.
//!
//! The socket lives in a directory only the current user can traverse, so the
//! OS already blocks other accounts; the token then decides what this user's
//! processes may reach.

use std::path::{Path, PathBuf};

/// Errors raised while locating or preparing the socket directory.
#[derive(Debug, thiserror::Error)]
pub enum SocketError {
    /// Neither `XDG_RUNTIME_DIR`, `XDG_CACHE_HOME`, nor `HOME` is set.
    #[error("no runtime directory is available for the Akooti socket")]
    NoRuntimeDir,
    /// The directory could not be created or locked down.
    #[error("the Akooti socket directory could not be prepared")]
    Io(#[source] std::io::Error),
}

/// Builds `<base>/akooti/daemon.sock`.
#[must_use]
pub fn socket_path_in(base: &Path) -> PathBuf {
    base.join("akooti").join("daemon.sock")
}

/// Resolves the default socket path.
///
/// `AKOOTI_SOCKET` wins, which is how tests and side-by-side daemons stay
/// isolated.
///
/// # Errors
///
/// Returns [`SocketError::NoRuntimeDir`] when no base directory can be found.
pub fn socket_path() -> Result<PathBuf, SocketError> {
    if let Some(override_path) = std::env::var_os("AKOOTI_SOCKET") {
        return Ok(PathBuf::from(override_path));
    }
    if let Some(runtime) = std::env::var_os("XDG_RUNTIME_DIR") {
        return Ok(socket_path_in(Path::new(&runtime)));
    }
    if let Some(cache) = std::env::var_os("XDG_CACHE_HOME") {
        return Ok(socket_path_in(Path::new(&cache)));
    }
    if let Some(home) = std::env::var_os("HOME") {
        return Ok(socket_path_in(&Path::new(&home).join(".cache")));
    }
    Err(SocketError::NoRuntimeDir)
}

/// Creates the socket directory with owner-only permissions.
///
/// # Errors
///
/// Returns [`SocketError::Io`] when the directory cannot be created or its mode
/// cannot be enforced.
pub fn ensure_socket_dir(path: &Path) -> Result<(), SocketError> {
    let directory = path.parent().unwrap_or(path);
    std::fs::create_dir_all(directory).map_err(SocketError::Io)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(directory, std::fs::Permissions::from_mode(0o700))
            .map_err(SocketError::Io)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{SocketError, socket_path_in};

    #[test]
    fn the_socket_lives_below_a_private_directory() {
        let path = socket_path_in(std::path::Path::new("/run/user/1000"));
        assert_eq!(
            path,
            std::path::PathBuf::from("/run/user/1000/akooti/daemon.sock")
        );
    }

    #[test]
    fn the_directory_is_created_owner_only() {
        let temp = tempfile::tempdir().expect("tempdir");
        let path = socket_path_in(temp.path());
        super::ensure_socket_dir(&path).expect("dir");
        assert!(path.parent().expect("parent").is_dir());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(path.parent().expect("parent"))
                .expect("metadata")
                .permissions()
                .mode()
                & 0o777;
            assert_eq!(mode, 0o700);
        }
    }

    #[test]
    fn errors_are_typed() {
        let error = SocketError::NoRuntimeDir;
        assert!(error.to_string().contains("runtime directory"));
    }
}
