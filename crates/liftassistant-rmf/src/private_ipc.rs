use std::{
    error::Error,
    fmt,
    fs::{self, DirBuilder, File, OpenOptions, Permissions},
    io::{self, Read, Seek, SeekFrom, Write},
    os::unix::{
        fs::{DirBuilderExt, FileTypeExt, MetadataExt, OpenOptionsExt, PermissionsExt},
        net::{UnixListener, UnixStream},
    },
    path::{Path, PathBuf},
    time::Duration,
};

use crate::{GatewayCommandRequest, GatewayCommandResponse};

const MAX_REQUEST_BYTES: u64 = 8 * 1024;
const CONNECTION_TIMEOUT: Duration = Duration::from_secs(2);

/// Process-held advisory lease for one concrete local RMF control domain.
///
/// The lease is deliberately local-host only. It prevents two gateway processes
/// from covering the same DDS-domain/native-lift pair through different socket
/// paths, but makes no cross-host split-brain claim.
pub struct LocalControlDomainLease {
    _lock: File,
    control_domain: String,
}

impl LocalControlDomainLease {
    pub fn acquire(control_domain: &str) -> Result<Self, PrivateIngressError> {
        let uid = rustix::process::getuid().as_raw();
        let run_user = PathBuf::from(format!("/run/user/{uid}"));
        let root = if secure_owned_directory(&run_user, uid) {
            run_user.join("liftassistant/control-domains")
        } else {
            std::env::temp_dir().join(format!("liftassistant-control-domains-{uid}"))
        };
        Self::acquire_in(&root, control_domain, uid)
    }

    fn acquire_in(
        root: &Path,
        control_domain: &str,
        uid: u32,
    ) -> Result<Self, PrivateIngressError> {
        if control_domain.is_empty() {
            return Err(PrivateIngressError::Io(
                "control-domain identity must not be empty".into(),
            ));
        }
        if !root.exists() {
            let mut builder = DirBuilder::new();
            builder.recursive(true).mode(0o700);
            builder.create(root)?;
        }
        let root_metadata = fs::symlink_metadata(root)?;
        if !root_metadata.is_dir()
            || root_metadata.file_type().is_symlink()
            || root_metadata.uid() != uid
            || root_metadata.permissions().mode() & 0o077 != 0
        {
            return Err(PrivateIngressError::Io(
                "control-domain lock directory must be owner-only and non-symlinked".into(),
            ));
        }
        let path = root.join(format!("{:016x}.lock", stable_domain_hash(control_domain)));
        if fs::symlink_metadata(&path).is_ok_and(|metadata| metadata.file_type().is_symlink()) {
            return Err(PrivateIngressError::Io(
                "control-domain lock path must not be a symbolic link".into(),
            ));
        }
        let mut lock = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .mode(0o600)
            .open(&path)?;
        let metadata = lock.metadata()?;
        if !metadata.is_file()
            || metadata.uid() != uid
            || metadata.permissions().mode() & 0o077 != 0
        {
            return Err(PrivateIngressError::Io(
                "control-domain lock file must be an owner-only regular file".into(),
            ));
        }
        rustix::fs::flock(&lock, rustix::fs::FlockOperation::NonBlockingLockExclusive).map_err(
            |error| {
                PrivateIngressError::Io(format!(
                    "RMF control domain {control_domain:?} is already active: {error}"
                ))
            },
        )?;
        let mut stored = String::new();
        lock.seek(SeekFrom::Start(0))?;
        lock.read_to_string(&mut stored)?;
        if stored.is_empty() {
            lock.seek(SeekFrom::Start(0))?;
            lock.write_all(control_domain.as_bytes())?;
            lock.write_all(b"\n")?;
            lock.sync_all()?;
            stored = format!("{control_domain}\n");
        }
        if stored.trim_end() != control_domain {
            return Err(PrivateIngressError::Io(
                "control-domain lock hash collision or stale identity mismatch".into(),
            ));
        }
        Ok(Self {
            _lock: lock,
            control_domain: control_domain.into(),
        })
    }

    pub fn control_domain(&self) -> &str {
        &self.control_domain
    }

    #[cfg(test)]
    pub(crate) fn acquire_for_test(
        root: &Path,
        control_domain: &str,
    ) -> Result<Self, PrivateIngressError> {
        Self::acquire_in(root, control_domain, rustix::process::getuid().as_raw())
    }
}

fn secure_owned_directory(path: &Path, uid: u32) -> bool {
    fs::symlink_metadata(path).is_ok_and(|metadata| {
        metadata.is_dir()
            && !metadata.file_type().is_symlink()
            && metadata.uid() == uid
            && metadata.permissions().mode() & 0o077 == 0
    })
}

fn stable_domain_hash(value: &str) -> u64 {
    value
        .as_bytes()
        .iter()
        .fold(0xcbf29ce484222325, |hash, byte| {
            (hash ^ u64::from(*byte)).wrapping_mul(0x100000001b3)
        })
}

#[derive(Debug)]
pub enum PrivateIngressError {
    Io(String),
    Oversized,
    InvalidRequest(String),
}

impl fmt::Display for PrivateIngressError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(message) => write!(f, "private gateway ingress I/O failed: {message}"),
            Self::Oversized => write!(
                f,
                "private gateway request exceeds {MAX_REQUEST_BYTES} bytes"
            ),
            Self::InvalidRequest(message) => {
                write!(f, "private gateway request is invalid: {message}")
            }
        }
    }
}

impl Error for PrivateIngressError {}

impl From<io::Error> for PrivateIngressError {
    fn from(value: io::Error) -> Self {
        Self::Io(value.to_string())
    }
}

/// A single-instance local-only ingress. Existing socket paths are never removed.
pub struct PrivateIngress {
    listener: UnixListener,
    _lock: File,
    path: PathBuf,
    device: u64,
    inode: u64,
}

impl PrivateIngress {
    pub fn bind(path: &Path) -> Result<Self, PrivateIngressError> {
        let parent = path.parent().ok_or_else(|| {
            PrivateIngressError::Io("private socket path has no parent directory".into())
        })?;
        if !parent.exists() {
            let mut builder = DirBuilder::new();
            builder.recursive(true).mode(0o700);
            builder.create(parent)?;
        }
        let socket_name = path
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| PrivateIngressError::Io("socket filename is not valid UTF-8".into()))?;
        let lock_path = parent.join(format!(".{socket_name}.lock"));
        let lock = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .mode(0o600)
            .open(lock_path)?;
        rustix::fs::flock(&lock, rustix::fs::FlockOperation::NonBlockingLockExclusive).map_err(
            |error| PrivateIngressError::Io(format!("gateway coverage is active: {error}")),
        )?;
        if let Ok(metadata) = fs::symlink_metadata(path) {
            if !metadata.file_type().is_socket() {
                return Err(PrivateIngressError::Io(
                    "private ingress path exists and is not a Unix socket".into(),
                ));
            }
            fs::remove_file(path)?;
        }
        let listener = UnixListener::bind(path)?;
        if let Err(error) = fs::set_permissions(path, Permissions::from_mode(0o600)) {
            let _ = fs::remove_file(path);
            return Err(error.into());
        }
        let metadata = fs::metadata(path)?;
        Ok(Self {
            listener,
            _lock: lock,
            path: path.to_owned(),
            device: metadata.dev(),
            inode: metadata.ino(),
        })
    }

    pub fn accept(&self) -> Result<PrivateIngressConnection, PrivateIngressError> {
        let (stream, _) = self.listener.accept()?;
        stream.set_read_timeout(Some(CONNECTION_TIMEOUT))?;
        stream.set_write_timeout(Some(CONNECTION_TIMEOUT))?;
        let credentials = rustix::net::sockopt::get_socket_peercred(&stream)
            .map_err(|error| PrivateIngressError::Io(error.to_string()))?;
        Ok(PrivateIngressConnection {
            peer_uid: credentials.uid.as_raw(),
            stream,
        })
    }
}

impl Drop for PrivateIngress {
    fn drop(&mut self) {
        if let Ok(metadata) = fs::metadata(&self.path) {
            if metadata.dev() == self.device && metadata.ino() == self.inode {
                let _ = fs::remove_file(&self.path);
            }
        }
    }
}

pub struct PrivateIngressConnection {
    peer_uid: u32,
    stream: UnixStream,
}

impl PrivateIngressConnection {
    pub const fn peer_uid(&self) -> u32 {
        self.peer_uid
    }

    pub fn read_request(&mut self) -> Result<GatewayCommandRequest, PrivateIngressError> {
        let mut bytes = Vec::new();
        Read::by_ref(&mut self.stream)
            .take(MAX_REQUEST_BYTES + 1)
            .read_to_end(&mut bytes)?;
        if bytes.len() as u64 > MAX_REQUEST_BYTES {
            return Err(PrivateIngressError::Oversized);
        }
        serde_json::from_slice(&bytes)
            .map_err(|error| PrivateIngressError::InvalidRequest(error.to_string()))
    }

    pub fn respond(mut self, response: &GatewayCommandResponse) -> Result<(), PrivateIngressError> {
        serde_json::to_writer(&mut self.stream, response)
            .map_err(|error| PrivateIngressError::Io(error.to_string()))?;
        self.stream.write_all(b"\n")?;
        self.stream.flush()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::{net::Shutdown, os::unix::net::UnixStream, thread};

    use tempfile::TempDir;

    use super::*;

    #[test]
    fn private_ingress_uses_kernel_peer_identity_and_bounded_json() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("private.sock");
        let ingress = PrivateIngress::bind(&path).unwrap();
        let worker = thread::spawn(move || {
            let mut connection = ingress.accept().unwrap();
            let uid = connection.peer_uid();
            let request = connection.read_request().unwrap();
            connection
                .respond(&GatewayCommandResponse {
                    accepted: true,
                    code: "fixture".into(),
                    intent_id: Some(7),
                    readiness: crate::GatewayReadiness::Ready,
                    recovery: None,
                })
                .unwrap();
            (uid, request)
        });
        let mut client = UnixStream::connect(&path).unwrap();
        client
            .write_all(br#"{"version":1,"operation":"serve_floor","logical_car":"A","floor":2}"#)
            .unwrap();
        client.shutdown(Shutdown::Write).unwrap();
        let mut response = String::new();
        client.read_to_string(&mut response).unwrap();
        assert!(response.contains("\"intent_id\":7"));
        let (uid, request) = worker.join().unwrap();
        assert_eq!(uid, rustix::process::getuid().as_raw());
        assert_eq!(request.floor, 2);
    }

    #[test]
    fn existing_socket_path_prevents_overlapping_gateway_coverage() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("private.sock");
        let first = PrivateIngress::bind(&path).unwrap();
        assert!(PrivateIngress::bind(&path).is_err());
        drop(first);
        assert!(PrivateIngress::bind(&path).is_ok());
    }

    #[test]
    fn control_domain_fences_different_socket_paths_and_releases_on_drop() {
        let temp = TempDir::new().unwrap();
        let root = temp.path().join("locks");
        let first = LocalControlDomainLease::acquire_for_test(&root, "rmf:47:Lift1").unwrap();
        assert!(LocalControlDomainLease::acquire_for_test(&root, "rmf:47:Lift1").is_err());
        let other = LocalControlDomainLease::acquire_for_test(&root, "rmf:47:Lift2").unwrap();
        drop(other);
        drop(first);
        assert!(LocalControlDomainLease::acquire_for_test(&root, "rmf:47:Lift1").is_ok());
    }
}
