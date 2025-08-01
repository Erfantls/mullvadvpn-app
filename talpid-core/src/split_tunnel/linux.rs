use std::{
    env, fs,
    io::{self, BufRead, BufReader, Write},
    path::{Path, PathBuf},
};
use talpid_types::{
    ErrorExt,
    cgroup::{SPLIT_TUNNEL_CGROUP_NAME, find_net_cls_mount},
};

const DEFAULT_NET_CLS_DIR: &str = "/sys/fs/cgroup/net_cls";
const NET_CLS_DIR_OVERRIDE_ENV_VAR: &str = "TALPID_NET_CLS_MOUNT_DIR";

/// Identifies packets coming from the cgroup.
/// This should be an arbitrary but unique integer.
pub const NET_CLS_CLASSID: u32 = 0x4d9f41;
/// Value used to mark packets and associated connections.
/// This should be an arbitrary but unique integer.
pub const MARK: i32 = 0xf41;

/// Errors related to split tunneling.
#[derive(thiserror::Error, Debug)]
pub enum Error {
    /// Unable to create cgroup.
    #[error("Unable to initialize net_cls cgroup instance")]
    InitNetClsCGroup(#[source] nix::Error),

    /// Unable to create cgroup.
    #[error("Unable to create cgroup for excluded processes")]
    CreateCGroup(#[source] io::Error),

    /// Split tunneling is unavailable
    #[error("Failed to set up split tunneling")]
    Unavailable,

    /// Unable to set class ID for cgroup.
    #[error("Unable to set cgroup class ID")]
    SetCGroupClassId(#[source] io::Error),

    /// Unable to add PID to cgroup.procs.
    #[error("Unable to add PID to cgroup.procs")]
    AddCGroupPid(#[source] io::Error),

    /// Unable to remove PID to cgroup.procs.
    #[error("Unable to remove PID from cgroup")]
    RemoveCGroupPid(#[source] io::Error),

    /// Unable to read cgroup.procs.
    #[error("Unable to obtain PIDs from cgroup.procs")]
    ListCGroupPids(#[source] io::Error),

    /// Unable to read /proc/mounts
    #[error("Failed to read /proc/mounts")]
    ListMounts(#[source] io::Error),
}

/// Manages PIDs in the Linux Cgroup excluded from the VPN tunnel.
pub struct PidManager {
    inner: Inner,
}

enum Inner {
    Ok { net_cls_path: PathBuf },
    Failed { err: Error },
}

impl Default for PidManager {
    /// Creates a new PID Cgroup manager.
    ///
    /// Finds the corresponding Cgroup to use. Will mount a `net_cls` filesystem
    /// if none exists.
    fn default() -> Self {
        let inner = match Self::new_inner() {
            Ok(net_cls_path) => Inner::Ok { net_cls_path },
            Err(err) => {
                log::error!(
                    "{}",
                    err.display_chain_with_msg("Failed to enable split tunneling")
                );
                Inner::Failed { err }
            }
        };
        PidManager { inner }
    }
}

impl PidManager {
    fn new_inner() -> Result<PathBuf, Error> {
        let net_cls_path = Self::create_cgroup()?;
        Self::setup_exclusion_group(&net_cls_path)?;
        Ok(net_cls_path)
    }

    /// Set up cgroup used to track PIDs for split tunneling.
    fn create_cgroup() -> Result<PathBuf, Error> {
        if let Some(net_cls_path) = find_net_cls_mount().map_err(Error::ListMounts)? {
            return Ok(net_cls_path);
        }

        let net_cls_dir = env::var(NET_CLS_DIR_OVERRIDE_ENV_VAR)
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from(DEFAULT_NET_CLS_DIR));

        if !net_cls_dir.exists() {
            fs::create_dir_all(&net_cls_dir).map_err(Error::CreateCGroup)?;
        }

        // https://www.kernel.org/doc/Documentation/cgroup-v1/net_cls.txt
        nix::mount::mount(
            Some("net_cls"),
            &net_cls_dir,
            Some("cgroup"),
            nix::mount::MsFlags::empty(),
            Some("net_cls"),
        )
        .map_err(Error::InitNetClsCGroup)?;

        Ok(net_cls_dir)
    }

    fn setup_exclusion_group(net_cls_path: &Path) -> Result<(), Error> {
        let exclusions_dir = net_cls_path.join(SPLIT_TUNNEL_CGROUP_NAME);
        if !exclusions_dir.exists() {
            fs::create_dir(exclusions_dir.clone()).map_err(Error::CreateCGroup)?;
        }

        let classid_path = exclusions_dir.join("net_cls.classid");
        fs::write(classid_path, NET_CLS_CLASSID.to_string().as_bytes())
            .map_err(Error::SetCGroupClassId)
    }

    fn get_net_cls_path(&self) -> Result<&Path, Error> {
        match &self.inner {
            Inner::Ok { net_cls_path } => Ok(net_cls_path),
            Inner::Failed { err } => {
                log::error!("Failed to get netcls path: {err}");
                Err(Error::Unavailable)
            }
        }
    }

    /// Add a PID to the Cgroup to have it excluded from the tunnel.
    pub fn add(&self, pid: i32) -> Result<(), Error> {
        let exclusions_path = self
            .get_net_cls_path()?
            .join(SPLIT_TUNNEL_CGROUP_NAME)
            .join("cgroup.procs");

        let mut file = fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(false)
            .open(exclusions_path)
            .map_err(Error::AddCGroupPid)?;

        file.write_all(pid.to_string().as_bytes())
            .map_err(Error::AddCGroupPid)
    }

    /// Remove a PID from the Cgroup to have it included in the tunnel.
    pub fn remove(&self, pid: i32) -> Result<(), Error> {
        // FIXME: We remove PIDs from our cgroup here by adding
        //        them to the parent cgroup. This seems wrong.
        let mut file = Self::open_parent_cgroup_handle(self.get_net_cls_path()?)
            .map_err(Error::RemoveCGroupPid)?;

        file.write_all(pid.to_string().as_bytes())
            .map_err(Error::RemoveCGroupPid)
    }

    /// Return a list of all PIDs currently in the Cgroup excluded from the tunnel.
    pub fn list(&self) -> Result<Vec<i32>, Error> {
        let exclusions_path = self
            .get_net_cls_path()?
            .join(SPLIT_TUNNEL_CGROUP_NAME)
            .join("cgroup.procs");

        let file = fs::File::open(exclusions_path).map_err(Error::ListCGroupPids)?;

        let result: Result<Vec<i32>, io::Error> = BufReader::new(file)
            .lines()
            .map(|line| {
                line.and_then(|v| {
                    v.parse()
                        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
                })
            })
            .collect();
        result.map_err(Error::ListCGroupPids)
    }

    /// Removes all PIDs from the Cgroup.
    pub fn clear(&self) -> Result<(), Error> {
        let pids = self.list()?;

        let mut file = Self::open_parent_cgroup_handle(self.get_net_cls_path()?)
            .map_err(Error::RemoveCGroupPid)?;
        for pid in pids {
            file.write_all(pid.to_string().as_bytes())
                .map_err(Error::RemoveCGroupPid)?;
        }

        Ok(())
    }

    /// Return whether it is enabled
    pub fn is_enabled(&self) -> bool {
        matches!(self.inner, Inner::Ok { .. })
    }

    fn open_parent_cgroup_handle(net_cls_path: &Path) -> io::Result<fs::File> {
        fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(false)
            .open(net_cls_path.join("cgroup.procs"))
    }
}
