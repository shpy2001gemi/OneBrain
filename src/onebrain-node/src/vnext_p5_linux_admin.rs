//! Closed Linux mutation backend for the P5 V2 qualification boundary.
//!
//! Callers select a typed operation. They cannot provide an executable,
//! interface, namespace, unit, qdisc, nft table, or shell fragment.

#[cfg(target_os = "linux")]
use std::process::Command;

use serde::{Deserialize, Serialize};

use crate::vnext_p5_multi_host_v2::P5FaultKindV2;

pub const P5_NAMESPACE: &str = "onebrain-p5-v2";
pub const P5_NAMESPACE_INTERFACE: &str = "obp5n0";
pub const P5_FAULT_TABLE: &str = "onebrain_p5_v2_fault";
pub const P5_AGENT_SERVICE: &str = "onebrain-p5-agent-v2.service";
pub const P5_IDENTITY_SOCKET: &str = "onebrain-p5-identity-signer-v2.socket";
pub const P5_IDENTITY_SERVICE: &str = "onebrain-p5-identity-signer-v2.service";
pub const P5_RECEIPT_SOCKET: &str = "onebrain-p5-receipt-signer-v2.socket";
pub const P5_RECEIPT_SERVICE: &str = "onebrain-p5-receipt-signer-v2.service";
pub const P5_RELAY_SERVICE: &str = "onebrain-relay-p5.service";
pub const P5_HOST_INTERFACE: &str = "obp5h0";
pub const P5_HOST_TABLE: &str = "onebrain_p5_v2_host";
pub const P5_NAT_TABLE: &str = "onebrain_p5_v2_nat";
const P5_NETWORK_STATE: &str = "/var/lib/onebrain/p5-v2/network-session.json";
const IPV4_FORWARD: &str = "/proc/sys/net/ipv4/ip_forward";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FixedCommandOutput {
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}

pub trait FixedLinuxCommandRunner {
    fn run(&self, program: &'static str, args: &[String]) -> Result<FixedCommandOutput, String>;
}

#[derive(Clone, Copy, Debug, Default)]
pub struct ProductionLinuxCommandRunner;

impl FixedLinuxCommandRunner for ProductionLinuxCommandRunner {
    fn run(&self, program: &'static str, args: &[String]) -> Result<FixedCommandOutput, String> {
        #[cfg(target_os = "linux")]
        {
            let output = Command::new(program)
                .args(args)
                .env_clear()
                .env("PATH", "/usr/sbin:/usr/bin:/sbin:/bin")
                .env("LC_ALL", "C")
                .output()
                .map_err(|error| format!("fixed operation could not start: {error}"))?;
            if !output.status.success() {
                return Err(format!(
                    "fixed operation failed rc={:?}: {}",
                    output.status.code(),
                    String::from_utf8_lossy(&output.stderr)
                ));
            }
            Ok(FixedCommandOutput {
                stdout: output.stdout,
                stderr: output.stderr,
            })
        }
        #[cfg(not(target_os = "linux"))]
        {
            let _ = (program, args);
            Err("P5 Linux admin backend requires Linux".into())
        }
    }
}

trait LinuxHostState {
    fn network_state_exists(&self) -> bool;
    fn namespace_exists(&self) -> bool;
    fn read_network_state(&self) -> Result<NetworkSessionState, String>;
    fn create_network_state(&self, state: &NetworkSessionState) -> Result<(), String>;
    fn replace_network_state(&self, state: &NetworkSessionState) -> Result<(), String>;
    fn remove_network_state(&self) -> Result<(), String>;
    fn read_ipv4_forwarding(&self) -> Result<String, String>;
    fn write_ipv4_forwarding(&self, value: &str) -> Result<(), String>;
    fn ufw_available(&self) -> bool;
}

#[derive(Clone, Copy, Debug, Default)]
struct ProductionLinuxHostState;

impl LinuxHostState for ProductionLinuxHostState {
    fn network_state_exists(&self) -> bool {
        std::path::Path::new(P5_NETWORK_STATE).exists()
    }

    fn namespace_exists(&self) -> bool {
        std::path::Path::new("/run/netns/onebrain-p5-v2").exists()
    }

    fn read_network_state(&self) -> Result<NetworkSessionState, String> {
        let bytes = std::fs::read(P5_NETWORK_STATE)
            .map_err(|error| format!("P5 network state is unavailable: {error}"))?;
        serde_json::from_slice(&bytes)
            .map_err(|error| format!("P5 network state is invalid: {error}"))
    }

    fn create_network_state(&self, state: &NetworkSessionState) -> Result<(), String> {
        create_durable_state(state)
    }

    fn replace_network_state(&self, state: &NetworkSessionState) -> Result<(), String> {
        replace_durable_state(state)
    }

    fn remove_network_state(&self) -> Result<(), String> {
        remove_durable_state()
    }

    fn read_ipv4_forwarding(&self) -> Result<String, String> {
        std::fs::read_to_string(IPV4_FORWARD)
            .map_err(|error| format!("cannot read IPv4 forwarding: {error}"))
    }

    fn write_ipv4_forwarding(&self, value: &str) -> Result<(), String> {
        std::fs::write(IPV4_FORWARD, value.as_bytes())
            .map_err(|error| format!("cannot write IPv4 forwarding: {error}"))
    }

    fn ufw_available(&self) -> bool {
        std::path::Path::new("/usr/sbin/ufw").is_file()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct LinuxFaultObservation {
    pub command_count: usize,
    pub stdout_blake3: String,
    pub stderr_blake3: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct NetworkSessionState {
    phase: NetworkSessionPhase,
    egress_interface: String,
    forwarding_was_enabled: bool,
    forwarding_changed: bool,
    ufw_active: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
enum NetworkSessionPhase {
    Preparing,
    Prepared,
    Cleaning,
}

pub struct P5LinuxAdminBackend<R> {
    runner: R,
    state: Box<dyn LinuxHostState>,
}

impl<R: FixedLinuxCommandRunner> P5LinuxAdminBackend<R> {
    pub fn new(runner: R) -> Self {
        Self {
            runner,
            state: Box::new(ProductionLinuxHostState),
        }
    }

    #[cfg(test)]
    fn with_state<S: LinuxHostState + 'static>(runner: R, state: S) -> Self {
        Self {
            runner,
            state: Box::new(state),
        }
    }

    pub fn observe(&self) -> Result<LinuxFaultObservation, String> {
        self.execute_commands(&[
            fixed_ip(&[
                "netns",
                "exec",
                P5_NAMESPACE,
                "tc",
                "-j",
                "qdisc",
                "show",
                "dev",
                P5_NAMESPACE_INTERFACE,
            ]),
            fixed_ip(&[
                "netns",
                "exec",
                P5_NAMESPACE,
                "nft",
                "-j",
                "list",
                "ruleset",
            ]),
            fixed_systemctl(&[
                "show",
                P5_AGENT_SERVICE,
                P5_IDENTITY_SOCKET,
                P5_IDENTITY_SERVICE,
                P5_RELAY_SERVICE,
                "--property=Id,LoadState,ActiveState,SubState,Result,MainPID",
                "--no-pager",
            ]),
        ])
    }

    /// Quiesce only the unprivileged agent process before a typed recovery
    /// operation opens its durable rollout/generation stores.  The immutable
    /// admin and receipt-signer boundaries remain available to attest the
    /// transition.
    pub fn quiesce_agent_for_recovery(&self) -> Result<LinuxFaultObservation, String> {
        self.execute_commands(&[fixed_systemctl(&["stop", P5_AGENT_SERVICE])])
    }

    /// Restart the immutable candidate agent after a recovery transition.  A
    /// durable rollback still fences network admission; starting the process
    /// cannot re-enable a killed generation.
    pub fn resume_agent_after_recovery(&self) -> Result<LinuxFaultObservation, String> {
        self.execute_commands(&[fixed_systemctl(&["start", P5_AGENT_SERVICE])])
    }

    pub fn prepare_session(&self) -> Result<LinuxFaultObservation, String> {
        if self.state.network_state_exists() || self.state.namespace_exists() {
            return Err("P5 network session already exists".into());
        }
        let route = self.runner.run(
            "/usr/sbin/ip",
            &["-j", "route", "get", "1.1.1.1"]
                .iter()
                .map(|value| (*value).into())
                .collect::<Vec<_>>(),
        )?;
        let egress = parse_egress_interface(&route.stdout)?;
        let forwarding = self.state.read_ipv4_forwarding()?;
        let forwarding_was_enabled = match forwarding.trim() {
            "0" => false,
            "1" => true,
            _ => return Err("IPv4 forwarding has an unsupported value".into()),
        };
        let ufw_active = if self.state.ufw_available() {
            let status = self.runner.run("/usr/sbin/ufw", &["status".into()])?;
            let value = String::from_utf8(status.stdout).map_err(|_| "UFW status is not UTF-8")?;
            if value.starts_with("Status: active") {
                true
            } else if value.starts_with("Status: inactive") {
                false
            } else {
                return Err("UFW returned an unknown status".into());
            }
        } else {
            false
        };
        let mut state = NetworkSessionState {
            phase: NetworkSessionPhase::Preparing,
            egress_interface: egress,
            forwarding_was_enabled,
            forwarding_changed: false,
            ufw_active,
        };
        // This write-ahead record is durable before the first host mutation. A
        // crash can therefore never leave an unowned namespace/NAT/sysctl
        // change that a later cleanup cannot identify.
        self.state.create_network_state(&state)?;
        if !forwarding_was_enabled {
            // Record ownership before touching the sysctl. If the write is
            // interrupted, cleanup conservatively restores the observed
            // original value instead of assuming that no mutation happened.
            state.forwarding_changed = true;
            self.state.replace_network_state(&state).map_err(|error| {
                format!("cannot durably reserve IPv4 forwarding ownership: {error}")
            })?;
            self.state.write_ipv4_forwarding("1\n")?;
            if self.state.read_ipv4_forwarding()?.trim() != "1" {
                let _ = self.state.write_ipv4_forwarding("0\n");
                return Err("IPv4 forwarding read-back failed".into());
            }
        }
        let commands = prepare_network_commands(&state.egress_interface, ufw_active);
        let observation = match self.execute_commands(&commands) {
            Ok(value) => value,
            Err(error) => {
                let rollback = self.rollback_preparation(&state);
                if rollback.is_ok() {
                    let _ = self.state.remove_network_state();
                }
                return Err(match rollback {
                    Ok(()) => error,
                    Err(rollback_error) => format!(
                        "{error}; rollback incomplete and durable recovery state retained: {rollback_error}"
                    ),
                });
            }
        };
        state.phase = NetworkSessionPhase::Prepared;
        if let Err(error) = self.state.replace_network_state(&state) {
            let rollback = self.rollback_preparation(&state);
            if rollback.is_ok() {
                let _ = self.state.remove_network_state();
            }
            return Err(match rollback {
                Ok(()) => format!("cannot publish prepared network state: {error}"),
                Err(rollback_error) => format!(
                    "cannot publish prepared network state: {error}; rollback incomplete and durable recovery state retained: {rollback_error}"
                ),
            });
        }
        Ok(observation)
    }

    pub fn cleanup_session(&self) -> Result<LinuxFaultObservation, String> {
        let mut state = self.state.read_network_state()?;
        validate_interface_name(&state.egress_interface)?;
        state.phase = NetworkSessionPhase::Cleaning;
        self.state
            .replace_network_state(&state)
            .map_err(|error| format!("cannot durably begin P5 network cleanup: {error}"))?;
        let observation = self.execute_commands_all(&cleanup_network_commands(
            &state.egress_interface,
            state.ufw_active,
        ))?;
        if state.forwarding_changed {
            self.state.write_ipv4_forwarding("0\n")?;
            if self.state.read_ipv4_forwarding()?.trim() != "0" {
                return Err("IPv4 forwarding restore read-back failed".into());
            }
        }
        self.state.remove_network_state()?;
        Ok(observation)
    }

    /// Finalize only after the controller has durably verified the signed
    /// cleanup receipt.  Network and agent state must already be absent; the
    /// receipt signer is intentionally the final service pair to stop.
    pub fn finalize_session(&self) -> Result<LinuxFaultObservation, String> {
        if self.state.network_state_exists() || self.state.namespace_exists() {
            return Err("P5 finalization requires a completed network cleanup".into());
        }
        self.execute_commands_all(&[
            fixed_systemctl(&["stop", P5_RECEIPT_SERVICE]),
            fixed_systemctl(&["stop", P5_RECEIPT_SOCKET]),
        ])
    }

    fn rollback_preparation(&self, state: &NetworkSessionState) -> Result<(), String> {
        let network_result = self
            .execute_commands_all(&rollback_network_commands(
                &state.egress_interface,
                state.ufw_active,
            ))
            .map(|_| ());
        let forwarding_result = if state.forwarding_changed {
            self.state.write_ipv4_forwarding("0\n").and_then(|_| {
                let value = self.state.read_ipv4_forwarding()?;
                if value.trim() == "0" {
                    Ok(())
                } else {
                    Err("IPv4 forwarding restore read-back failed".into())
                }
            })
        } else {
            Ok(())
        };
        match (network_result, forwarding_result) {
            (Ok(()), Ok(())) => Ok(()),
            (Err(network), Ok(())) => Err(network),
            (Ok(()), Err(forwarding)) => Err(forwarding),
            (Err(network), Err(forwarding)) => Err(format!("{network}; {forwarding}")),
        }
    }

    pub fn apply(
        &self,
        fault: P5FaultKindV2,
        peer_endpoints: &[String],
    ) -> Result<LinuxFaultObservation, String> {
        let commands = match fault {
            P5FaultKindV2::Partition => partition_commands(peer_endpoints)?,
            P5FaultKindV2::Drop => vec![netem(&["loss", "10%"])],
            P5FaultKindV2::Reorder => vec![netem(&["delay", "20ms", "reorder", "25%", "50%"])],
            P5FaultKindV2::Duplicate => vec![netem(&["duplicate", "5%"])],
            P5FaultKindV2::SlowPeer => vec![netem(&["delay", "250ms", "25ms", "rate", "512kbit"])],
            P5FaultKindV2::Restart => vec![fixed_systemctl(&["restart", P5_AGENT_SERVICE])],
            P5FaultKindV2::AddressChange => vec![
                fixed_ip(&[
                    "-n",
                    P5_NAMESPACE,
                    "addr",
                    "del",
                    "10.254.28.2/29",
                    "dev",
                    P5_NAMESPACE_INTERFACE,
                ]),
                fixed_ip(&[
                    "-n",
                    P5_NAMESPACE,
                    "addr",
                    "add",
                    "10.254.28.3/29",
                    "dev",
                    P5_NAMESPACE_INTERFACE,
                ]),
            ],
            P5FaultKindV2::SeedOutage | P5FaultKindV2::SelectedRelayShutdown => {
                vec![fixed_systemctl(&["stop", P5_RELAY_SERVICE])]
            }
            P5FaultKindV2::SignerOutage => vec![
                fixed_systemctl(&["stop", P5_IDENTITY_SOCKET]),
                fixed_systemctl(&["stop", P5_IDENTITY_SERVICE]),
            ],
            P5FaultKindV2::DiskPressure
            | P5FaultKindV2::BaseObarv002ArchiveRestore
            | P5FaultKindV2::Rollback
            | P5FaultKindV2::ExplicitReEnable => {
                return Err("fault requires the separate closed storage/recovery backend".into())
            }
        };
        self.execute_commands(&commands)
    }

    pub fn clear(&self, fault: P5FaultKindV2) -> Result<LinuxFaultObservation, String> {
        let commands = match fault {
            P5FaultKindV2::Partition => vec![fixed_ip(&[
                "netns",
                "exec",
                P5_NAMESPACE,
                "nft",
                "delete",
                "table",
                "inet",
                P5_FAULT_TABLE,
            ])],
            P5FaultKindV2::Drop
            | P5FaultKindV2::Reorder
            | P5FaultKindV2::Duplicate
            | P5FaultKindV2::SlowPeer => vec![fixed_ip(&[
                "netns",
                "exec",
                P5_NAMESPACE,
                "tc",
                "qdisc",
                "del",
                "dev",
                P5_NAMESPACE_INTERFACE,
                "root",
            ])],
            P5FaultKindV2::Restart => vec![fixed_systemctl(&["start", P5_AGENT_SERVICE])],
            P5FaultKindV2::AddressChange => vec![
                fixed_ip(&[
                    "-n",
                    P5_NAMESPACE,
                    "addr",
                    "del",
                    "10.254.28.3/29",
                    "dev",
                    P5_NAMESPACE_INTERFACE,
                ]),
                fixed_ip(&[
                    "-n",
                    P5_NAMESPACE,
                    "addr",
                    "add",
                    "10.254.28.2/29",
                    "dev",
                    P5_NAMESPACE_INTERFACE,
                ]),
            ],
            P5FaultKindV2::SeedOutage | P5FaultKindV2::SelectedRelayShutdown => {
                vec![fixed_systemctl(&["start", P5_RELAY_SERVICE])]
            }
            P5FaultKindV2::SignerOutage => vec![
                fixed_systemctl(&["start", P5_IDENTITY_SOCKET]),
                fixed_systemctl(&["start", P5_IDENTITY_SERVICE]),
            ],
            P5FaultKindV2::DiskPressure
            | P5FaultKindV2::BaseObarv002ArchiveRestore
            | P5FaultKindV2::Rollback
            | P5FaultKindV2::ExplicitReEnable => {
                return Err("fault requires the separate closed storage/recovery backend".into())
            }
        };
        self.execute_commands(&commands)
    }

    fn execute_commands(
        &self,
        commands: &[(&'static str, Vec<String>)],
    ) -> Result<LinuxFaultObservation, String> {
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        for (program, args) in commands {
            let output = self.runner.run(program, args)?;
            stdout.extend_from_slice(&output.stdout);
            stderr.extend_from_slice(&output.stderr);
        }
        Ok(LinuxFaultObservation {
            command_count: commands.len(),
            stdout_blake3: blake3::hash(&stdout).to_hex().to_string(),
            stderr_blake3: blake3::hash(&stderr).to_hex().to_string(),
        })
    }

    fn execute_commands_all(
        &self,
        commands: &[(&'static str, Vec<String>)],
    ) -> Result<LinuxFaultObservation, String> {
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let mut first_error = None;
        for (program, args) in commands {
            match self.runner.run(program, args) {
                Ok(output) => {
                    stdout.extend_from_slice(&output.stdout);
                    stderr.extend_from_slice(&output.stderr);
                }
                Err(error) => {
                    stderr.extend_from_slice(error.as_bytes());
                    if first_error.is_none() {
                        first_error = Some(error);
                    }
                }
            }
        }
        if let Some(error) = first_error {
            return Err(format!(
                "cleanup completed with an operation failure: {error}"
            ));
        }
        Ok(LinuxFaultObservation {
            command_count: commands.len(),
            stdout_blake3: blake3::hash(&stdout).to_hex().to_string(),
            stderr_blake3: blake3::hash(&stderr).to_hex().to_string(),
        })
    }
}

fn fixed_ip(args: &[&str]) -> (&'static str, Vec<String>) {
    (
        "/usr/sbin/ip",
        args.iter().map(|value| (*value).to_owned()).collect(),
    )
}

fn fixed_systemctl(args: &[&str]) -> (&'static str, Vec<String>) {
    (
        "/usr/bin/systemctl",
        args.iter().map(|value| (*value).to_owned()).collect(),
    )
}

fn fixed_nft(args: &[&str]) -> (&'static str, Vec<String>) {
    (
        "/usr/sbin/nft",
        args.iter().map(|value| (*value).to_owned()).collect(),
    )
}

fn fixed_ufw(args: &[&str]) -> (&'static str, Vec<String>) {
    (
        "/usr/sbin/ufw",
        args.iter().map(|value| (*value).to_owned()).collect(),
    )
}

fn parse_egress_interface(bytes: &[u8]) -> Result<String, String> {
    let rows: Vec<serde_json::Value> =
        serde_json::from_slice(bytes).map_err(|_| "route selection is not canonical JSON")?;
    if rows.len() != 1 {
        return Err("route selection is ambiguous".into());
    }
    let value = rows[0]
        .get("dev")
        .and_then(serde_json::Value::as_str)
        .ok_or("route selection has no egress interface")?
        .to_owned();
    validate_interface_name(&value)?;
    Ok(value)
}

fn validate_interface_name(value: &str) -> Result<(), String> {
    if value.is_empty()
        || value.len() > 15
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.'))
    {
        return Err("route-selected egress interface is invalid".into());
    }
    Ok(())
}

fn prepare_network_commands(egress: &str, ufw_active: bool) -> Vec<(&'static str, Vec<String>)> {
    let mut commands = vec![
        fixed_systemctl(&["start", P5_RECEIPT_SOCKET]),
        fixed_systemctl(&["start", P5_RECEIPT_SERVICE]),
        fixed_ip(&["netns", "add", P5_NAMESPACE]),
        fixed_ip(&[
            "link",
            "add",
            P5_HOST_INTERFACE,
            "type",
            "veth",
            "peer",
            "name",
            P5_NAMESPACE_INTERFACE,
        ]),
        fixed_ip(&["link", "set", P5_NAMESPACE_INTERFACE, "netns", P5_NAMESPACE]),
        fixed_ip(&["addr", "add", "10.254.28.1/29", "dev", P5_HOST_INTERFACE]),
        fixed_ip(&["link", "set", P5_HOST_INTERFACE, "up"]),
        fixed_ip(&["-n", P5_NAMESPACE, "link", "set", "lo", "up"]),
        fixed_ip(&[
            "-n",
            P5_NAMESPACE,
            "addr",
            "add",
            "10.254.28.2/29",
            "dev",
            P5_NAMESPACE_INTERFACE,
        ]),
        fixed_ip(&[
            "-n",
            P5_NAMESPACE,
            "link",
            "set",
            P5_NAMESPACE_INTERFACE,
            "up",
        ]),
        fixed_ip(&[
            "-n",
            P5_NAMESPACE,
            "route",
            "add",
            "default",
            "via",
            "10.254.28.1",
        ]),
        fixed_nft(&["add", "table", "ip", P5_NAT_TABLE]),
        fixed_nft(&[
            "add",
            "chain",
            "ip",
            P5_NAT_TABLE,
            "postrouting",
            "{",
            "type",
            "nat",
            "hook",
            "postrouting",
            "priority",
            "srcnat",
            ";",
            "policy",
            "accept",
            ";",
            "}",
        ]),
    ];
    commands.push((
        "/usr/sbin/nft",
        vec![
            "add",
            "rule",
            "ip",
            P5_NAT_TABLE,
            "postrouting",
            "iifname",
            P5_HOST_INTERFACE,
            "oifname",
            egress,
            "ip",
            "saddr",
            "10.254.28.0/29",
            "counter",
            "masquerade",
            "comment",
            "onebrain-p5-v2-nat",
        ]
        .into_iter()
        .map(str::to_owned)
        .collect(),
    ));
    if ufw_active {
        commands.push((
            "/usr/sbin/ufw",
            vec![
                "route",
                "allow",
                "in",
                "on",
                P5_HOST_INTERFACE,
                "out",
                "on",
                egress,
                "from",
                "10.254.28.0/29",
                "comment",
                "onebrain-p5-v2-route",
            ]
            .into_iter()
            .map(str::to_owned)
            .collect(),
        ));
        commands.push(fixed_ufw(&[
            "allow",
            "in",
            "on",
            P5_HOST_INTERFACE,
            "proto",
            "udp",
            "from",
            "10.254.28.0/29",
            "to",
            "10.254.28.1",
            "port",
            "41000",
            "comment",
            "onebrain-p5-v2-relay-udp",
        ]));
        commands.push(fixed_ufw(&[
            "allow",
            "in",
            "on",
            P5_HOST_INTERFACE,
            "proto",
            "tcp",
            "from",
            "10.254.28.0/29",
            "to",
            "10.254.28.1",
            "port",
            "443",
            "comment",
            "onebrain-p5-v2-relay-tcp",
        ]));
    } else {
        commands.extend([
            fixed_nft(&["add", "table", "inet", P5_HOST_TABLE]),
            fixed_nft(&[
                "add",
                "chain",
                "inet",
                P5_HOST_TABLE,
                "forward",
                "{",
                "type",
                "filter",
                "hook",
                "forward",
                "priority",
                "-5",
                ";",
                "policy",
                "accept",
                ";",
                "}",
            ]),
            fixed_nft(&[
                "add",
                "chain",
                "inet",
                P5_HOST_TABLE,
                "input",
                "{",
                "type",
                "filter",
                "hook",
                "input",
                "priority",
                "-5",
                ";",
                "policy",
                "accept",
                ";",
                "}",
            ]),
        ]);
    }
    commands.extend([
        fixed_systemctl(&["start", P5_IDENTITY_SOCKET]),
        fixed_systemctl(&["start", P5_IDENTITY_SERVICE]),
        fixed_systemctl(&["start", "onebrain-p5-agent-v2.socket"]),
        fixed_systemctl(&["start", P5_AGENT_SERVICE]),
    ]);
    commands
}

fn cleanup_network_commands(egress: &str, ufw_active: bool) -> Vec<(&'static str, Vec<String>)> {
    let mut commands = vec![
        fixed_systemctl(&["stop", P5_AGENT_SERVICE]),
        fixed_systemctl(&["stop", "onebrain-p5-agent-v2.socket"]),
        fixed_systemctl(&["stop", P5_IDENTITY_SERVICE]),
        fixed_systemctl(&["stop", P5_IDENTITY_SOCKET]),
    ];
    if ufw_active {
        commands.push((
            "/usr/sbin/ufw",
            vec![
                "--force",
                "delete",
                "route",
                "allow",
                "in",
                "on",
                P5_HOST_INTERFACE,
                "out",
                "on",
                egress,
                "from",
                "10.254.28.0/29",
                "comment",
                "onebrain-p5-v2-route",
            ]
            .into_iter()
            .map(str::to_owned)
            .collect(),
        ));
        commands.push(fixed_ufw(&[
            "--force",
            "delete",
            "allow",
            "in",
            "on",
            P5_HOST_INTERFACE,
            "proto",
            "udp",
            "from",
            "10.254.28.0/29",
            "to",
            "10.254.28.1",
            "port",
            "41000",
            "comment",
            "onebrain-p5-v2-relay-udp",
        ]));
        commands.push(fixed_ufw(&[
            "--force",
            "delete",
            "allow",
            "in",
            "on",
            P5_HOST_INTERFACE,
            "proto",
            "tcp",
            "from",
            "10.254.28.0/29",
            "to",
            "10.254.28.1",
            "port",
            "443",
            "comment",
            "onebrain-p5-v2-relay-tcp",
        ]));
    } else {
        commands.push(fixed_nft(&["delete", "table", "inet", P5_HOST_TABLE]));
    }
    commands.extend([
        fixed_nft(&["delete", "table", "ip", P5_NAT_TABLE]),
        fixed_ip(&["netns", "delete", P5_NAMESPACE]),
    ]);
    commands
}

fn rollback_network_commands(egress: &str, ufw_active: bool) -> Vec<(&'static str, Vec<String>)> {
    let mut commands = cleanup_network_commands(egress, ufw_active);
    // A failed prepare never yielded a signed lifecycle receipt, so its
    // prerequisite signer must not remain active. Normal cleanup deliberately
    // omits these two operations until the separately verified finalization.
    commands.extend([
        fixed_systemctl(&["stop", P5_RECEIPT_SERVICE]),
        fixed_systemctl(&["stop", P5_RECEIPT_SOCKET]),
    ]);
    commands
}

fn create_durable_state(state: &NetworkSessionState) -> Result<(), String> {
    use std::io::Write;
    let path = std::path::Path::new(P5_NETWORK_STATE);
    let parent = path.parent().ok_or("network state has no parent")?;
    std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path).map_err(|error| error.to_string())?;
    file.write_all(&serde_json::to_vec(state).map_err(|error| error.to_string())?)
        .map_err(|error| error.to_string())?;
    file.sync_all().map_err(|error| error.to_string())?;
    std::fs::File::open(parent)
        .and_then(|value| value.sync_all())
        .map_err(|error| error.to_string())
}

fn replace_durable_state(state: &NetworkSessionState) -> Result<(), String> {
    use std::io::Write;
    let path = std::path::Path::new(P5_NETWORK_STATE);
    let parent = path.parent().ok_or("network state has no parent")?;
    let next = parent.join("network-session.json.next");
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(&next).map_err(|error| error.to_string())?;
    let result = (|| {
        file.write_all(&serde_json::to_vec(state).map_err(|error| error.to_string())?)
            .map_err(|error| error.to_string())?;
        file.sync_all().map_err(|error| error.to_string())?;
        std::fs::rename(&next, path).map_err(|error| error.to_string())?;
        std::fs::File::open(parent)
            .and_then(|value| value.sync_all())
            .map_err(|error| error.to_string())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&next);
    }
    result
}

fn remove_durable_state() -> Result<(), String> {
    let path = std::path::Path::new(P5_NETWORK_STATE);
    let parent = path.parent().ok_or("network state has no parent")?;
    std::fs::remove_file(path).map_err(|error| error.to_string())?;
    std::fs::File::open(parent)
        .and_then(|value| value.sync_all())
        .map_err(|error| error.to_string())
}

fn netem(policy: &[&str]) -> (&'static str, Vec<String>) {
    let mut args = [
        "netns",
        "exec",
        P5_NAMESPACE,
        "tc",
        "qdisc",
        "replace",
        "dev",
        P5_NAMESPACE_INTERFACE,
        "root",
        "netem",
    ]
    .iter()
    .map(|value| (*value).to_owned())
    .collect::<Vec<_>>();
    args.extend(policy.iter().map(|value| (*value).to_owned()));
    ("/usr/sbin/ip", args)
}

fn partition_commands(
    peer_endpoints: &[String],
) -> Result<Vec<(&'static str, Vec<String>)>, String> {
    if peer_endpoints.is_empty() || peer_endpoints.len() > 8 {
        return Err("partition endpoint set is outside the closed bound".into());
    }
    let mut commands = vec![
        fixed_ip(&[
            "netns",
            "exec",
            P5_NAMESPACE,
            "nft",
            "add",
            "table",
            "inet",
            P5_FAULT_TABLE,
        ]),
        fixed_ip(&[
            "netns",
            "exec",
            P5_NAMESPACE,
            "nft",
            "add",
            "chain",
            "inet",
            P5_FAULT_TABLE,
            "output",
            "{",
            "type",
            "filter",
            "hook",
            "output",
            "priority",
            "filter",
            ";",
            "policy",
            "accept",
            ";",
            "}",
        ]),
    ];
    for value in peer_endpoints {
        let endpoint: std::net::SocketAddr =
            value.parse().map_err(|_| "invalid signed peer endpoint")?;
        if !is_globally_routable(endpoint.ip()) {
            return Err("partition endpoint is not globally routable".into());
        }
        let (family, address) = match endpoint.ip() {
            std::net::IpAddr::V4(value) => ("ip", value.to_string()),
            std::net::IpAddr::V6(value) => ("ip6", value.to_string()),
        };
        commands.push(fixed_ip(&[
            "netns",
            "exec",
            P5_NAMESPACE,
            "nft",
            "add",
            "rule",
            "inet",
            P5_FAULT_TABLE,
            "output",
            family,
            "daddr",
            &address,
            "udp",
            "dport",
            &endpoint.port().to_string(),
            "counter",
            "drop",
            "comment",
            "onebrain-p5-v2-partition",
        ]));
    }
    Ok(commands)
}

fn is_globally_routable(address: std::net::IpAddr) -> bool {
    match address {
        std::net::IpAddr::V4(value) => {
            let octets = value.octets();
            !(value.is_unspecified()
                || value.is_loopback()
                || value.is_private()
                || value.is_link_local()
                || value.is_multicast()
                || value.is_broadcast()
                || octets[0] == 0
                || octets[0] >= 240
                || (octets[0] == 100 && (64..=127).contains(&octets[1]))
                || (octets[0] == 192 && octets[1] == 0 && octets[2] <= 2)
                || (octets[0] == 198 && (octets[1] == 18 || octets[1] == 19 || octets[1] == 51))
                || (octets[0] == 203 && octets[1] == 0 && octets[2] == 113))
        }
        std::net::IpAddr::V6(value) => {
            let segments = value.segments();
            !(value.is_unspecified()
                || value.is_loopback()
                || value.is_multicast()
                || (segments[0] & 0xfe00) == 0xfc00
                || (segments[0] & 0xffc0) == 0xfe80
                || (segments[0] == 0x2001 && segments[1] == 0x0db8))
        }
    }
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
                stdout: b"ok\n".to_vec(),
                stderr: Vec::new(),
            })
        }
    }

    #[derive(Clone)]
    struct FakeHostState(Arc<Mutex<FakeHostInner>>);

    #[derive(Clone)]
    struct FakeHostInner {
        network_state: Option<NetworkSessionState>,
        namespace_present: bool,
        forwarding: String,
        ufw_available: bool,
    }

    impl FakeHostState {
        fn new(forwarding: &str) -> Self {
            Self(Arc::new(Mutex::new(FakeHostInner {
                network_state: None,
                namespace_present: false,
                forwarding: forwarding.into(),
                ufw_available: false,
            })))
        }
    }

    impl LinuxHostState for FakeHostState {
        fn network_state_exists(&self) -> bool {
            self.0.lock().unwrap().network_state.is_some()
        }

        fn namespace_exists(&self) -> bool {
            self.0.lock().unwrap().namespace_present
        }

        fn read_network_state(&self) -> Result<NetworkSessionState, String> {
            self.0
                .lock()
                .unwrap()
                .network_state
                .clone()
                .ok_or_else(|| "missing fake network state".into())
        }

        fn create_network_state(&self, state: &NetworkSessionState) -> Result<(), String> {
            let mut inner = self.0.lock().unwrap();
            if inner.network_state.is_some() {
                return Err("fake create-new collision".into());
            }
            inner.network_state = Some(state.clone());
            Ok(())
        }

        fn replace_network_state(&self, state: &NetworkSessionState) -> Result<(), String> {
            let mut inner = self.0.lock().unwrap();
            if inner.network_state.is_none() {
                return Err("fake state is unavailable".into());
            }
            inner.network_state = Some(state.clone());
            Ok(())
        }

        fn remove_network_state(&self) -> Result<(), String> {
            self.0.lock().unwrap().network_state = None;
            Ok(())
        }

        fn read_ipv4_forwarding(&self) -> Result<String, String> {
            Ok(self.0.lock().unwrap().forwarding.clone())
        }

        fn write_ipv4_forwarding(&self, value: &str) -> Result<(), String> {
            self.0.lock().unwrap().forwarding = value.into();
            Ok(())
        }

        fn ufw_available(&self) -> bool {
            self.0.lock().unwrap().ufw_available
        }
    }

    struct FaultRunner {
        calls: Mutex<Vec<(&'static str, Vec<String>)>>,
        fail_fragments: Mutex<Vec<String>>,
    }

    impl FaultRunner {
        fn new(fail_fragments: &[&str]) -> Self {
            Self {
                calls: Mutex::new(Vec::new()),
                fail_fragments: Mutex::new(
                    fail_fragments.iter().map(|value| (*value).into()).collect(),
                ),
            }
        }
    }

    impl FixedLinuxCommandRunner for FaultRunner {
        fn run(
            &self,
            program: &'static str,
            args: &[String],
        ) -> Result<FixedCommandOutput, String> {
            self.calls.lock().unwrap().push((program, args.to_vec()));
            let rendered = format!("{program} {}", args.join(" "));
            let mut failures = self.fail_fragments.lock().unwrap();
            if let Some(index) = failures
                .iter()
                .position(|fragment| rendered.contains(fragment))
            {
                failures.remove(index);
                return Err(format!("injected failure: {rendered}"));
            }
            let stdout = if args == ["-j", "route", "get", "1.1.1.1"] {
                br#"[{"dev":"ens160"}]"#.to_vec()
            } else {
                b"ok\n".to_vec()
            };
            Ok(FixedCommandOutput {
                stdout,
                stderr: Vec::new(),
            })
        }
    }

    #[test]
    fn network_faults_use_only_fixed_namespace_interface_and_literals() {
        let backend = P5LinuxAdminBackend::new(RecordingRunner::default());
        assert_eq!(
            backend
                .apply(P5FaultKindV2::Drop, &[])
                .unwrap()
                .command_count,
            1
        );
        let calls = backend.runner.0.lock().unwrap();
        assert_eq!(calls[0].0, "/usr/sbin/ip");
        assert_eq!(
            calls[0].1.join(" "),
            "netns exec onebrain-p5-v2 tc qdisc replace dev obp5n0 root netem loss 10%"
        );
    }

    #[test]
    fn signer_outage_stops_socket_before_service_and_restores_in_order() {
        let backend = P5LinuxAdminBackend::new(RecordingRunner::default());
        backend.apply(P5FaultKindV2::SignerOutage, &[]).unwrap();
        backend.clear(P5FaultKindV2::SignerOutage).unwrap();
        let calls = backend.runner.0.lock().unwrap();
        assert_eq!(calls[0].1, vec!["stop", P5_IDENTITY_SOCKET]);
        assert_eq!(calls[1].1, vec!["stop", P5_IDENTITY_SERVICE]);
        assert_eq!(calls[2].1, vec!["start", P5_IDENTITY_SOCKET]);
        assert_eq!(calls[3].1, vec!["start", P5_IDENTITY_SERVICE]);
    }

    #[test]
    fn partition_is_bounded_global_and_never_accepts_private_or_text_rules() {
        let backend = P5LinuxAdminBackend::new(RecordingRunner::default());
        assert!(backend
            .apply(P5FaultKindV2::Partition, &["10.0.0.1:41000".into()])
            .is_err());
        assert!(backend
            .apply(P5FaultKindV2::Partition, &["not-an-endpoint".into()])
            .is_err());
        assert_eq!(
            backend
                .apply(P5FaultKindV2::Partition, &["1.1.1.1:41000".into()])
                .unwrap()
                .command_count,
            3
        );
    }

    #[test]
    fn lifecycle_commands_are_closed_and_ufw_absence_never_requires_ufw() {
        assert_eq!(
            parse_egress_interface(br#"[{"dev":"ens160"}]"#).unwrap(),
            "ens160"
        );
        assert!(parse_egress_interface(br#"[{"dev":"ens160;rm"}]"#).is_err());
        let inactive = prepare_network_commands("ens160", false);
        assert_eq!(inactive[0].1, vec!["start", P5_RECEIPT_SOCKET]);
        assert_eq!(inactive[1].1, vec!["start", P5_RECEIPT_SERVICE]);
        assert!(inactive
            .iter()
            .all(|(program, _)| *program != "/usr/sbin/ufw"));
        assert!(inactive
            .iter()
            .any(|(_, args)| args.iter().any(|value| value == P5_NAT_TABLE)));
        let active = prepare_network_commands("ens160", true);
        assert_eq!(
            active
                .iter()
                .filter(|(program, _)| *program == "/usr/sbin/ufw")
                .count(),
            3
        );
        let cleanup = cleanup_network_commands("ens160", true);
        assert_eq!(
            cleanup
                .iter()
                .filter(|(program, _)| *program == "/usr/sbin/ufw")
                .count(),
            3
        );
        assert_eq!(
            cleanup.last().unwrap().1,
            vec!["netns", "delete", P5_NAMESPACE]
        );
        assert!(cleanup.iter().all(|(_, args)| {
            !args
                .iter()
                .any(|value| value == P5_RECEIPT_SOCKET || value == P5_RECEIPT_SERVICE)
        }));
        let rollback = rollback_network_commands("ens160", true);
        assert_eq!(
            rollback[rollback.len() - 2].1,
            vec!["stop", P5_RECEIPT_SERVICE]
        );
        assert_eq!(rollback.last().unwrap().1, vec!["stop", P5_RECEIPT_SOCKET]);
    }

    #[test]
    fn failed_prepare_rolls_back_owned_forwarding_and_removes_state_only_after_success() {
        let host = FakeHostState::new("0\n");
        let backend = P5LinuxAdminBackend::with_state(
            FaultRunner::new(&["netns add onebrain-p5-v2"]),
            host.clone(),
        );
        assert!(backend.prepare_session().is_err());
        let inner = host.0.lock().unwrap();
        assert_eq!(inner.forwarding, "0\n");
        assert!(inner.network_state.is_none());
    }

    #[test]
    fn incomplete_prepare_rollback_retains_durable_recovery_state() {
        let host = FakeHostState::new("0\n");
        let backend = P5LinuxAdminBackend::with_state(
            FaultRunner::new(&[
                "netns add onebrain-p5-v2",
                "nft delete table ip onebrain_p5_v2_nat",
            ]),
            host.clone(),
        );
        let error = backend.prepare_session().unwrap_err();
        assert!(error.contains("rollback incomplete"));
        let inner = host.0.lock().unwrap();
        let state = inner.network_state.as_ref().unwrap();
        assert_eq!(state.phase, NetworkSessionPhase::Preparing);
        assert!(state.forwarding_changed);
        assert_eq!(inner.forwarding, "0\n");
    }

    #[test]
    fn cleanup_failure_keeps_cleaning_state_and_success_removes_it_after_sysctl_restore() {
        let host = FakeHostState::new("1\n");
        host.0.lock().unwrap().network_state = Some(NetworkSessionState {
            phase: NetworkSessionPhase::Prepared,
            egress_interface: "ens160".into(),
            forwarding_was_enabled: false,
            forwarding_changed: true,
            ufw_active: false,
        });
        let failed = P5LinuxAdminBackend::with_state(
            FaultRunner::new(&["nft delete table ip onebrain_p5_v2_nat"]),
            host.clone(),
        );
        assert!(failed.cleanup_session().is_err());
        {
            let inner = host.0.lock().unwrap();
            assert_eq!(inner.forwarding, "1\n");
            assert_eq!(
                inner.network_state.as_ref().unwrap().phase,
                NetworkSessionPhase::Cleaning
            );
        }

        let resumed = P5LinuxAdminBackend::with_state(FaultRunner::new(&[]), host.clone());
        assert!(resumed.cleanup_session().is_ok());
        let inner = host.0.lock().unwrap();
        assert_eq!(inner.forwarding, "0\n");
        assert!(inner.network_state.is_none());
    }

    #[test]
    fn finalization_is_blocked_until_cleanup_then_stops_receipt_service_before_socket() {
        let dirty = FakeHostState::new("1\n");
        dirty.0.lock().unwrap().network_state = Some(NetworkSessionState {
            phase: NetworkSessionPhase::Prepared,
            egress_interface: "ens160".into(),
            forwarding_was_enabled: true,
            forwarding_changed: false,
            ufw_active: false,
        });
        let blocked = P5LinuxAdminBackend::with_state(RecordingRunner::default(), dirty);
        assert!(blocked.finalize_session().is_err());

        let clean = FakeHostState::new("1\n");
        let backend = P5LinuxAdminBackend::with_state(RecordingRunner::default(), clean);
        assert_eq!(backend.finalize_session().unwrap().command_count, 2);
        let calls = backend.runner.0.lock().unwrap();
        assert_eq!(calls[0].1, vec!["stop", P5_RECEIPT_SERVICE]);
        assert_eq!(calls[1].1, vec!["stop", P5_RECEIPT_SOCKET]);
    }
}
