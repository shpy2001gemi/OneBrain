//! Durable, closed ENOSPC fault backend for P5 V2.
//!
//! The fixed 512 MiB image is allocated before baseline measurement.  The
//! fault phase only mounts and fills that image, so it cannot consume the VPS
//! root filesystem while the fault is active.  No caller supplies a path,
//! size, filesystem, mount option, or executable.

use std::fs::{File, OpenOptions};
use std::io::{ErrorKind, Write};
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::vnext_p5_linux_admin::{
    FixedCommandOutput, FixedLinuxCommandRunner, LinuxFaultObservation,
};

pub const P5_ENOSPC_IMAGE_BYTES: u64 = 536_870_912;
const P5_FAULT_ROOT: &str = "/var/lib/onebrain/p5-v2-fault";
const P5_ENOSPC_IMAGE: &str = "/var/lib/onebrain/p5-v2-fault/enospc.img";
const P5_ENOSPC_MOUNT: &str = "/var/lib/onebrain/p5-v2-fault/enospc";
const P5_ENOSPC_FILL: &str = "/var/lib/onebrain/p5-v2-fault/enospc/fill.bin";
const P5_ENOSPC_STATE: &str = "/var/lib/onebrain/p5-v2-fault/enospc-state.json";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
enum DiskPressurePhase {
    Preparing,
    Prepared,
    Mounting,
    Mounted,
    Cleaning,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct DiskPressureState {
    phase: DiskPressurePhase,
    image_bytes: u64,
}

trait DiskPressureHost {
    fn state_exists(&self) -> bool;
    fn read_state(&self) -> Result<DiskPressureState, String>;
    fn create_state(&self, state: &DiskPressureState) -> Result<(), String>;
    fn replace_state(&self, state: &DiskPressureState) -> Result<(), String>;
    fn remove_state(&self) -> Result<(), String>;
    fn create_image_and_mountpoint(&self) -> Result<(), String>;
    fn fill_until_enospc(&self) -> Result<u64, String>;
    fn remove_image_and_mountpoint(&self) -> Result<(), String>;
}

#[derive(Clone, Copy, Debug, Default)]
struct ProductionDiskPressureHost;

impl DiskPressureHost for ProductionDiskPressureHost {
    fn state_exists(&self) -> bool {
        Path::new(P5_ENOSPC_STATE).exists()
    }

    fn read_state(&self) -> Result<DiskPressureState, String> {
        let bytes = std::fs::read(P5_ENOSPC_STATE).map_err(|error| error.to_string())?;
        serde_json::from_slice(&bytes).map_err(|error| error.to_string())
    }

    fn create_state(&self, state: &DiskPressureState) -> Result<(), String> {
        write_state_create_new(state)
    }

    fn replace_state(&self, state: &DiskPressureState) -> Result<(), String> {
        write_state_replace(state)
    }

    fn remove_state(&self) -> Result<(), String> {
        std::fs::remove_file(P5_ENOSPC_STATE).map_err(|error| error.to_string())?;
        sync_parent(Path::new(P5_ENOSPC_STATE))
    }

    fn create_image_and_mountpoint(&self) -> Result<(), String> {
        create_private_dir(Path::new(P5_FAULT_ROOT))?;
        create_private_dir(Path::new(P5_ENOSPC_MOUNT))?;
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let image = options
            .open(P5_ENOSPC_IMAGE)
            .map_err(|error| error.to_string())?;
        image
            .set_len(P5_ENOSPC_IMAGE_BYTES)
            .and_then(|_| image.sync_all())
            .map_err(|error| error.to_string())?;
        sync_parent(Path::new(P5_ENOSPC_IMAGE))
    }

    fn fill_until_enospc(&self) -> Result<u64, String> {
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options
            .open(P5_ENOSPC_FILL)
            .map_err(|error| error.to_string())?;
        let block = [0xA5u8; 1_048_576];
        let mut written = 0u64;
        loop {
            match file.write_all(&block) {
                Ok(()) => {
                    written = written
                        .checked_add(block.len() as u64)
                        .ok_or("ENOSPC byte counter overflow")?;
                    if written > P5_ENOSPC_IMAGE_BYTES {
                        return Err("ENOSPC image accepted bytes beyond its fixed bound".into());
                    }
                }
                Err(error)
                    if error.raw_os_error() == Some(28)
                        || error.kind() == ErrorKind::StorageFull =>
                {
                    file.sync_all().map_err(|error| error.to_string())?;
                    return Ok(written);
                }
                Err(error) => return Err(error.to_string()),
            }
        }
    }

    fn remove_image_and_mountpoint(&self) -> Result<(), String> {
        match std::fs::remove_file(P5_ENOSPC_FILL) {
            Ok(()) => {}
            Err(error) if error.kind() == ErrorKind::NotFound => {}
            Err(error) => return Err(error.to_string()),
        }
        match std::fs::remove_file(P5_ENOSPC_IMAGE) {
            Ok(()) => {}
            Err(error) if error.kind() == ErrorKind::NotFound => {}
            Err(error) => return Err(error.to_string()),
        }
        match std::fs::remove_dir(P5_ENOSPC_MOUNT) {
            Ok(()) => {}
            Err(error) if error.kind() == ErrorKind::NotFound => {}
            Err(error) => return Err(error.to_string()),
        }
        sync_parent(Path::new(P5_ENOSPC_IMAGE))
    }
}

pub struct P5DiskPressureBackend<R> {
    runner: R,
    host: Box<dyn DiskPressureHost>,
}

impl<R: FixedLinuxCommandRunner> P5DiskPressureBackend<R> {
    pub fn new(runner: R) -> Self {
        Self {
            runner,
            host: Box::new(ProductionDiskPressureHost),
        }
    }

    #[cfg(test)]
    fn with_host<H: DiskPressureHost + 'static>(runner: R, host: H) -> Self {
        Self {
            runner,
            host: Box::new(host),
        }
    }

    pub fn prepare(&self) -> Result<LinuxFaultObservation, String> {
        if self.host.state_exists() {
            return Err("P5 ENOSPC image state already exists".into());
        }
        let mut state = DiskPressureState {
            phase: DiskPressurePhase::Preparing,
            image_bytes: P5_ENOSPC_IMAGE_BYTES,
        };
        self.host.create_state(&state)?;
        if let Err(error) = self.host.create_image_and_mountpoint() {
            return Err(format!(
                "ENOSPC image preparation failed; durable state retained: {error}"
            ));
        }
        let result = self.run_commands(&[fixed_fallocate(), fixed_mkfs()]);
        let observation = match result {
            Ok(value) => value,
            Err(error) => {
                let _ = self.host.remove_image_and_mountpoint();
                return Err(format!(
                    "ENOSPC image formatting failed; durable state retained: {error}"
                ));
            }
        };
        state.phase = DiskPressurePhase::Prepared;
        self.host.replace_state(&state)?;
        Ok(observation)
    }

    pub fn apply(&self) -> Result<LinuxFaultObservation, String> {
        let mut state = self.host.read_state()?;
        if state.phase != DiskPressurePhase::Prepared || state.image_bytes != P5_ENOSPC_IMAGE_BYTES
        {
            return Err("ENOSPC image is not in the prepared state".into());
        }
        state.phase = DiskPressurePhase::Mounting;
        self.host.replace_state(&state)?;
        let mount = self.run_commands(&[fixed_mount()])?;
        let written = match self.host.fill_until_enospc() {
            Ok(value) => value,
            Err(error) => {
                let unmount = self.run_commands(&[fixed_umount()]);
                if unmount.is_ok() {
                    state.phase = DiskPressurePhase::Prepared;
                    let _ = self.host.replace_state(&state);
                }
                return Err(format!(
                    "ENOSPC fill did not reach the closed condition: {error}"
                ));
            }
        };
        if written == 0 || written > P5_ENOSPC_IMAGE_BYTES {
            return Err("ENOSPC fill byte count is outside the closed bound".into());
        }
        state.phase = DiskPressurePhase::Mounted;
        self.host.replace_state(&state)?;
        let mut bytes = mount.stdout_blake3.into_bytes();
        bytes.extend_from_slice(&written.to_be_bytes());
        Ok(LinuxFaultObservation {
            command_count: mount.command_count,
            stdout_blake3: blake3::hash(&bytes).to_hex().to_string(),
            stderr_blake3: mount.stderr_blake3,
        })
    }

    pub fn clear(&self) -> Result<LinuxFaultObservation, String> {
        let mut state = self.host.read_state()?;
        if !matches!(
            state.phase,
            DiskPressurePhase::Mounting | DiskPressurePhase::Mounted | DiskPressurePhase::Cleaning
        ) {
            return Err("ENOSPC image is not mounted".into());
        }
        state.phase = DiskPressurePhase::Cleaning;
        self.host.replace_state(&state)?;
        let observation = self.run_commands(&[fixed_umount()])?;
        state.phase = DiskPressurePhase::Prepared;
        self.host.replace_state(&state)?;
        Ok(observation)
    }

    pub fn cleanup(&self) -> Result<LinuxFaultObservation, String> {
        let mut state = self.host.read_state()?;
        let mut outputs = Vec::new();
        if matches!(
            state.phase,
            DiskPressurePhase::Mounting | DiskPressurePhase::Mounted | DiskPressurePhase::Cleaning
        ) {
            state.phase = DiskPressurePhase::Cleaning;
            self.host.replace_state(&state)?;
            outputs.push(self.run_commands(&[fixed_umount()])?);
        }
        self.host.remove_image_and_mountpoint()?;
        self.host.remove_state()?;
        Ok(combine_observations(outputs))
    }

    fn run_commands(
        &self,
        commands: &[(&'static str, Vec<String>)],
    ) -> Result<LinuxFaultObservation, String> {
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        for (program, args) in commands {
            let output: FixedCommandOutput = self.runner.run(program, args)?;
            stdout.extend_from_slice(&output.stdout);
            stderr.extend_from_slice(&output.stderr);
        }
        Ok(LinuxFaultObservation {
            command_count: commands.len(),
            stdout_blake3: blake3::hash(&stdout).to_hex().to_string(),
            stderr_blake3: blake3::hash(&stderr).to_hex().to_string(),
        })
    }
}

fn fixed_fallocate() -> (&'static str, Vec<String>) {
    (
        "/usr/bin/fallocate",
        vec![
            "--length".into(),
            P5_ENOSPC_IMAGE_BYTES.to_string(),
            P5_ENOSPC_IMAGE.into(),
        ],
    )
}

fn fixed_mkfs() -> (&'static str, Vec<String>) {
    (
        "/usr/sbin/mkfs.ext4",
        vec![
            "-q".into(),
            "-F".into(),
            "-m".into(),
            "0".into(),
            P5_ENOSPC_IMAGE.into(),
        ],
    )
}

fn fixed_mount() -> (&'static str, Vec<String>) {
    (
        "/usr/bin/mount",
        vec![
            "-o".into(),
            "loop,noexec,nodev,nosuid".into(),
            P5_ENOSPC_IMAGE.into(),
            P5_ENOSPC_MOUNT.into(),
        ],
    )
}

fn fixed_umount() -> (&'static str, Vec<String>) {
    ("/usr/bin/umount", vec![P5_ENOSPC_MOUNT.into()])
}

fn combine_observations(values: Vec<LinuxFaultObservation>) -> LinuxFaultObservation {
    let command_count = values.iter().map(|value| value.command_count).sum();
    let bytes = values
        .iter()
        .flat_map(|value| {
            [
                value.stdout_blake3.as_bytes(),
                value.stderr_blake3.as_bytes(),
            ]
        })
        .flatten()
        .copied()
        .collect::<Vec<_>>();
    LinuxFaultObservation {
        command_count,
        stdout_blake3: blake3::hash(&bytes).to_hex().to_string(),
        stderr_blake3: blake3::hash(b"").to_hex().to_string(),
    }
}

fn create_private_dir(path: &Path) -> Result<(), String> {
    std::fs::create_dir_all(path).map_err(|error| error.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}

fn write_state_create_new(state: &DiskPressureState) -> Result<(), String> {
    create_private_dir(Path::new(P5_FAULT_ROOT))?;
    write_state_file(Path::new(P5_ENOSPC_STATE), state, true)
}

fn write_state_replace(state: &DiskPressureState) -> Result<(), String> {
    let next = Path::new(P5_FAULT_ROOT).join("enospc-state.json.next");
    write_state_file(&next, state, true)?;
    std::fs::rename(&next, P5_ENOSPC_STATE).map_err(|error| error.to_string())?;
    sync_parent(Path::new(P5_ENOSPC_STATE))
}

fn write_state_file(
    path: &Path,
    state: &DiskPressureState,
    create_new: bool,
) -> Result<(), String> {
    let mut options = OpenOptions::new();
    options
        .write(true)
        .create_new(create_new)
        .truncate(!create_new);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path).map_err(|error| error.to_string())?;
    file.write_all(&serde_json::to_vec(state).map_err(|error| error.to_string())?)
        .and_then(|_| file.sync_all())
        .map_err(|error| error.to_string())
}

fn sync_parent(path: &Path) -> Result<(), String> {
    let parent = path.parent().ok_or("path has no parent")?;
    File::open(parent)
        .and_then(|file| file.sync_all())
        .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    #[derive(Default)]
    struct RecordingRunner(Mutex<Vec<(&'static str, Vec<String>)>>);

    impl FixedLinuxCommandRunner for RecordingRunner {
        fn run(
            &self,
            program: &'static str,
            args: &[String],
        ) -> Result<FixedCommandOutput, String> {
            self.0.lock().unwrap().push((program, args.to_vec()));
            Ok(FixedCommandOutput {
                stdout: b"ok".to_vec(),
                stderr: Vec::new(),
            })
        }
    }

    #[derive(Clone, Default)]
    struct FakeHost(Arc<Mutex<Option<DiskPressureState>>>);

    impl DiskPressureHost for FakeHost {
        fn state_exists(&self) -> bool {
            self.0.lock().unwrap().is_some()
        }
        fn read_state(&self) -> Result<DiskPressureState, String> {
            self.0
                .lock()
                .unwrap()
                .clone()
                .ok_or_else(|| "missing".into())
        }
        fn create_state(&self, state: &DiskPressureState) -> Result<(), String> {
            let mut value = self.0.lock().unwrap();
            if value.is_some() {
                return Err("collision".into());
            }
            *value = Some(state.clone());
            Ok(())
        }
        fn replace_state(&self, state: &DiskPressureState) -> Result<(), String> {
            *self.0.lock().unwrap() = Some(state.clone());
            Ok(())
        }
        fn remove_state(&self) -> Result<(), String> {
            *self.0.lock().unwrap() = None;
            Ok(())
        }
        fn create_image_and_mountpoint(&self) -> Result<(), String> {
            Ok(())
        }
        fn fill_until_enospc(&self) -> Result<u64, String> {
            Ok(P5_ENOSPC_IMAGE_BYTES - 4096)
        }
        fn remove_image_and_mountpoint(&self) -> Result<(), String> {
            Ok(())
        }
    }

    #[test]
    fn disk_pressure_is_preallocated_then_mounted_filled_and_cleaned_with_fixed_commands() {
        let host = FakeHost::default();
        let backend = P5DiskPressureBackend::with_host(RecordingRunner::default(), host.clone());
        assert_eq!(backend.prepare().unwrap().command_count, 2);
        assert_eq!(
            host.read_state().unwrap().phase,
            DiskPressurePhase::Prepared
        );
        assert_eq!(backend.apply().unwrap().command_count, 1);
        assert_eq!(host.read_state().unwrap().phase, DiskPressurePhase::Mounted);
        assert_eq!(backend.clear().unwrap().command_count, 1);
        assert_eq!(
            host.read_state().unwrap().phase,
            DiskPressurePhase::Prepared
        );
        assert_eq!(backend.cleanup().unwrap().command_count, 0);
        assert!(!host.state_exists());
        let calls = backend.runner.0.lock().unwrap();
        assert_eq!(calls[0], fixed_fallocate());
        assert_eq!(calls[1], fixed_mkfs());
        assert_eq!(calls[2], fixed_mount());
        assert_eq!(calls[3], fixed_umount());
    }
}
