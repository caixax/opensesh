//! The remote monitor (PLAN Sprint 11): what the status bar shows about a server (CPU, memory,
//! network, disks, uptime, load, users) and the host info (OS, kernel, architecture, host name,
//! CPUs, addresses), without installing anything there and never in the user's shell.
//!
//! - **One exec channel per monitor:** a POSIX `sh` loop prints a reading every few seconds, until
//!   the channel closes (the next `echo` fails and `sh` ends). The host info is a one-shot command.
//! - **Linux** (busybox too) reads `/proc` through the shell's own `read`, so a reading starts no
//!   process for those files; `df` and `who` are the only programs it runs, with `sleep`.
//!   **FreeBSD** and **macOS** use `sysctl`, `route`, `netstat -ibn`, `vm_stat` and `ps`. Anything
//!   else gives what it can, or nothing ([`Monitoring::Unsupported`]).
//! - **Any login shell:** the script runs under `sh -c '...'`, on one line, without single quotes
//!   or `!`, so bash, zsh, fish and csh all pass it to `sh` unchanged.
//! - **Rates** (CPU use, network bytes per second) come from two readings ([`Rates`]).
//! - **Network traffic** is that of the interface of the default route, so a bridge, a container's
//!   interface or a VPN doesn't count the same bytes twice.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use russh::ChannelMsg;

use crate::connect::Connection;

/// `os`, and `f`, which prints a file through the shell's `read` (no process).
const PRELUDE: &str = r#"os=$(uname -s); f() { while read -r l; do echo "$l"; done < "$1"; }"#;

/// One reading: sections from `@os` to `@end`.
const READING: &str = concat!(
    r#"echo "@os $os"; "#,
    r#"if [ -r /proc/stat ]; then echo @cpu; read -r l < /proc/stat; echo "$l"; "#,
    r#"echo @meminfo; f /proc/meminfo; echo @netdev; f /proc/net/dev; echo @route; f /proc/net/route; "#,
    r#"echo @uptime; f /proc/uptime; echo @loadavg; f /proc/loadavg; "#,
    r#"else echo @sysctl; for k in hw.ncpu hw.physmem hw.memsize hw.pagesize "#,
    r#"vm.stats.vm.v_free_count vm.stats.vm.v_inactive_count vm.stats.vm.v_laundry_count "#,
    r#"kern.cp_time vm.loadavg kern.boottime vm.swapusage; "#,
    r#"do v=$(sysctl -n $k 2>/dev/null) && echo "$k $v"; done; echo "now $(date +%s)"; "#,
    r#"echo @route; route -n get default 2>/dev/null; echo @netstat; netstat -ibn 2>/dev/null; "#,
    r#"if [ "$os" = Darwin ]; then echo @vmstat; vm_stat; echo @pscpu; ps -A -o %cpu=; fi; fi; "#,
    r#"echo @df; df -Pk 2>/dev/null; echo @who; who 2>/dev/null; echo @end"#,
);

/// The host info's own sections, before a reading.
const INFO: &str = concat!(
    r#"echo @release; [ -r /etc/os-release ] && f /etc/os-release; "#,
    r#"echo @uname; uname -srm; echo @hostname; hostname 2>/dev/null || uname -n; "#,
    r#"echo @ncpu; nproc 2>/dev/null || getconf _NPROCESSORS_ONLN 2>/dev/null || sysctl -n hw.ncpu 2>/dev/null; "#,
    r#"echo @addr; ip -o addr show 2>/dev/null || ifconfig -a 2>/dev/null; "#,
    r#"if [ "$os" = Darwin ]; then echo @swvers; sw_vers 2>/dev/null; fi"#,
);

/// The shortest and longest interval between readings.
const INTERVALS: (u64, u64) = (1, 3600);

/// How long the first reading may take (a slow login, a first PAM session).
const FIRST_READING: Duration = Duration::from_secs(20);

/// How long the host info may take.
const INFO_TIMEOUT: Duration = Duration::from_secs(20);

/// Output kept while waiting for the end of a reading: more means it isn't our command talking.
const MAX_PENDING: usize = 256 * 1024;

/// The command that prints a reading every `interval` (1 s to an hour).
#[must_use]
pub fn command(interval: Duration) -> String {
    let seconds = interval.as_secs().clamp(INTERVALS.0, INTERVALS.1);
    format!("sh -c 'n={seconds}; {PRELUDE}; while :; do {READING}; sleep $n; done'")
}

/// The command that prints the host info, and one reading.
#[must_use]
pub fn info_command() -> String {
    format!("sh -c '{PRELUDE}; {INFO}; {READING}'")
}

/// CPU time counters: busy and total ticks since boot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CpuTicks {
    /// Ticks not idle.
    pub busy: u64,
    /// Every tick.
    pub total: u64,
}

/// Memory and swap, in bytes.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Memory {
    /// Installed.
    pub total: u64,
    /// Free for programs (free, and caches the kernel gives back).
    pub available: u64,
    /// Swap space (0 without swap, or where it isn't known).
    pub swap_total: u64,
    /// Swap space free.
    pub swap_free: u64,
}

/// Bytes through the network since boot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Traffic {
    /// Received.
    pub received: u64,
    /// Sent.
    pub sent: u64,
}

/// A mounted file system, sizes in bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Disk {
    /// The device or dataset (`/dev/sda1`, `zroot/ROOT/default`).
    pub filesystem: String,
    /// Where it is mounted.
    pub mount: String,
    /// Its size.
    pub size: u64,
    /// Used.
    pub used: u64,
    /// Free for users.
    pub available: u64,
}

/// A logged-in user session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct User {
    /// The user name.
    pub name: String,
    /// The terminal line (`pts/0`, `console`).
    pub line: String,
    /// Where from (a host or an address), when `who` says.
    pub from: String,
}

/// What one reading of the command says (counters, not rates).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Reading {
    /// The kernel name (`uname -s`).
    pub os: String,
    /// CPU counters, where the system has them (Linux, FreeBSD).
    pub cpu: Option<CpuTicks>,
    /// CPU use in percent, where only that is known (macOS).
    pub cpu_percent: Option<f64>,
    /// Memory and swap.
    pub memory: Option<Memory>,
    /// Network counters of the default route's interface (or of every other interface).
    pub traffic: Option<Traffic>,
    /// Mounted file systems, pseudo file systems left out.
    pub disks: Vec<Disk>,
    /// Seconds since boot.
    pub uptime: Option<u64>,
    /// The load average over 1, 5 and 15 minutes.
    pub load: Option<[f64; 3]>,
    /// Logged-in users.
    pub users: Vec<User>,
}

impl Reading {
    /// Whether it says nothing a status bar could show.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.cpu.is_none()
            && self.cpu_percent.is_none()
            && self.memory.is_none()
            && self.traffic.is_none()
            && self.disks.is_empty()
            && self.uptime.is_none()
            && self.load.is_none()
    }
}

/// What the status bar and the Info tab show: a reading, with rates from the one before.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Snapshot {
    /// CPU use, in thousandths (0 to 1000).
    pub cpu_permille: Option<u16>,
    /// Memory and swap.
    pub memory: Option<Memory>,
    /// Bytes received per second.
    pub received_per_sec: Option<u64>,
    /// Bytes sent per second.
    pub sent_per_sec: Option<u64>,
    /// Mounted file systems.
    pub disks: Vec<Disk>,
    /// Seconds since boot.
    pub uptime_secs: Option<u64>,
    /// The load average over 1, 5 and 15 minutes, in hundredths.
    pub load_hundredths: Option<[u32; 3]>,
    /// Logged-in users.
    pub users: Vec<User>,
}

impl Snapshot {
    /// The disk mounted at `/`, else the first one.
    #[must_use]
    pub fn root_disk(&self) -> Option<&Disk> {
        self.disks
            .iter()
            .find(|disk| disk.mount == "/")
            .or_else(|| self.disks.first())
    }
}

/// Turns readings into snapshots, with rates from the reading before.
#[derive(Debug, Default)]
pub struct Rates {
    previous: Option<(Instant, Option<CpuTicks>, Option<Traffic>)>,
}

impl Rates {
    /// The snapshot of `reading`, taken at `at`.
    pub fn next(&mut self, reading: &Reading, at: Instant) -> Snapshot {
        let (cpu_permille, received_per_sec, sent_per_sec) = match self.previous {
            Some((then, cpu, traffic)) => {
                let cpu = match (cpu, reading.cpu) {
                    (Some(before), Some(now)) => {
                        let total = now.total.checked_sub(before.total).filter(|t| *t > 0);
                        let busy = now.busy.checked_sub(before.busy);
                        total.zip(busy).map(|(total, busy)| {
                            u16::try_from(busy.min(total).saturating_mul(1000) / total)
                                .unwrap_or(1000)
                        })
                    }
                    _ => None,
                };
                let seconds = at.duration_since(then).as_secs_f64();
                let rate = |before: u64, now: u64| {
                    (seconds > 0.0)
                        .then(|| now.checked_sub(before))
                        .flatten()
                        .map(|bytes| (bytes as f64 / seconds).round() as u64)
                };
                let (received, sent) = match (traffic, reading.traffic) {
                    (Some(before), Some(now)) => (
                        rate(before.received, now.received),
                        rate(before.sent, now.sent),
                    ),
                    _ => (None, None),
                };
                (cpu, received, sent)
            }
            None => (None, None, None),
        };
        self.previous = Some((at, reading.cpu, reading.traffic));
        let percent = reading
            .cpu_percent
            .map(|percent| (percent.clamp(0.0, 100.0) * 10.0).round() as u16);
        Snapshot {
            cpu_permille: cpu_permille.or(percent),
            memory: reading.memory,
            received_per_sec,
            sent_per_sec,
            disks: reading.disks.clone(),
            uptime_secs: reading.uptime,
            load_hundredths: reading
                .load
                .map(|load| load.map(|value| (value.max(0.0) * 100.0).round() as u32)),
            users: reading.users.clone(),
        }
    }
}

/// The sections of `output`: `@name` lines start them (text after the name is the first line).
fn sections(output: &str) -> HashMap<&str, Vec<&str>> {
    let mut out: HashMap<&str, Vec<&str>> = HashMap::new();
    let mut current: Option<&str> = None;
    for line in output.lines() {
        let line = line.trim_end_matches('\r');
        if let Some(rest) = line.strip_prefix('@')
            && !rest.is_empty()
            && rest
                .bytes()
                .next()
                .is_some_and(|byte| byte.is_ascii_lowercase())
        {
            let (name, value) = rest.split_once(' ').unwrap_or((rest, ""));
            let lines = out.entry(name).or_default();
            if !value.is_empty() {
                lines.push(value);
            }
            current = Some(name);
        } else if let Some(name) = current {
            out.entry(name).or_default().push(line);
        }
    }
    out
}

fn numbers<T: std::str::FromStr>(text: &str) -> Vec<T> {
    text.split_whitespace()
        .filter_map(|word| word.parse().ok())
        .collect()
}

/// What one reading of [`command`] printed (text before the first section is ignored).
#[must_use]
pub fn parse(output: &str) -> Reading {
    let sections = sections(output);
    let section = |name: &str| sections.get(name).map(Vec::as_slice).unwrap_or_default();
    let sysctl: HashMap<&str, &str> = section("sysctl")
        .iter()
        .filter_map(|line| line.split_once(' '))
        .collect();
    let os = section("os")
        .first()
        .map(|os| os.trim().to_owned())
        .unwrap_or_default();
    Reading {
        cpu: linux_cpu(section("cpu")).or_else(|| bsd_cpu(&sysctl)),
        cpu_percent: mac_cpu(section("pscpu"), &sysctl),
        memory: linux_memory(section("meminfo"))
            .or_else(|| mac_memory(section("vmstat"), &sysctl))
            .or_else(|| bsd_memory(&sysctl)),
        traffic: linux_traffic(section("netdev"), section("route"))
            .or_else(|| bsd_traffic(section("netstat"), section("route"))),
        disks: disks(section("df")),
        uptime: section("uptime")
            .first()
            .and_then(|line| numbers::<f64>(line).first().copied())
            .map(|seconds| seconds.max(0.0) as u64)
            .or_else(|| bsd_uptime(&sysctl)),
        load: section("loadavg")
            .first()
            .and_then(|line| load(line))
            .or_else(|| sysctl.get("vm.loadavg").and_then(|line| load(line))),
        users: users(section("who")),
        os,
    }
}

/// The first three numbers of a load average line (`0.10 0.20 0.30 1/234 5678`,
/// `{ 0.10 0.20 0.30 }`).
fn load(line: &str) -> Option<[f64; 3]> {
    let values: Vec<f64> = numbers(line);
    Some([*values.first()?, *values.get(1)?, *values.get(2)?])
}

/// `cpu  user nice system idle iowait irq softirq steal ...` of `/proc/stat`.
fn linux_cpu(lines: &[&str]) -> Option<CpuTicks> {
    let line = lines.iter().find(|line| line.starts_with("cpu "))?;
    let values: Vec<u64> = numbers(line);
    // guest and guest_nice are already counted in user and nice.
    let counted = &values[..values.len().min(8)];
    let total = sum(counted.iter().copied());
    let idle = values
        .get(3)?
        .saturating_add(values.get(4).copied().unwrap_or(0));
    Some(CpuTicks {
        busy: total.saturating_sub(idle),
        total,
    })
}

/// The sum of a server's counters, at most `u64::MAX` (a server can print any number).
fn sum(values: impl IntoIterator<Item = u64>) -> u64 {
    values.into_iter().fold(0, u64::saturating_add)
}

/// `kern.cp_time`: user, nice, system, interrupt and idle ticks (FreeBSD).
fn bsd_cpu(sysctl: &HashMap<&str, &str>) -> Option<CpuTicks> {
    let values: Vec<u64> = numbers(sysctl.get("kern.cp_time")?);
    let idle = *values.get(4)?;
    let total = sum(values.iter().take(5).copied());
    Some(CpuTicks {
        busy: total.saturating_sub(idle),
        total,
    })
}

/// The CPU use of every process (`ps -A -o %cpu=`) over the number of CPUs (macOS).
fn mac_cpu(lines: &[&str], sysctl: &HashMap<&str, &str>) -> Option<f64> {
    if lines.is_empty() {
        return None;
    }
    let cpus = sysctl
        .get("hw.ncpu")
        .and_then(|value| value.trim().parse::<f64>().ok())
        .filter(|cpus| *cpus >= 1.0)
        .unwrap_or(1.0);
    let sum: f64 = lines
        .iter()
        .filter_map(|line| line.trim().replace(',', ".").parse::<f64>().ok())
        .sum();
    Some((sum / cpus).clamp(0.0, 100.0))
}

/// `/proc/meminfo` (values in kB).
fn linux_memory(lines: &[&str]) -> Option<Memory> {
    let values: HashMap<&str, u64> = lines
        .iter()
        .filter_map(|line| {
            let (key, value) = line.split_once(':')?;
            Some((key.trim(), *numbers::<u64>(value).first()?))
        })
        .collect();
    let kb = |key: &str| values.get(key).map(|value| value.saturating_mul(1024));
    let total = kb("MemTotal")?;
    // Kernels before 3.14 have no MemAvailable.
    let available = kb("MemAvailable")
        .unwrap_or_else(|| sum(["MemFree", "Buffers", "Cached"].map(|key| kb(key).unwrap_or(0))));
    Some(Memory {
        total,
        available: available.min(total),
        swap_total: kb("SwapTotal").unwrap_or(0),
        swap_free: kb("SwapFree").unwrap_or(0),
    })
}

fn sysctl_number(sysctl: &HashMap<&str, &str>, key: &str) -> Option<u64> {
    sysctl.get(key).and_then(|value| value.trim().parse().ok())
}

/// FreeBSD: installed memory, and free and inactive pages.
fn bsd_memory(sysctl: &HashMap<&str, &str>) -> Option<Memory> {
    let total = sysctl_number(sysctl, "hw.physmem")?;
    let page = sysctl_number(sysctl, "hw.pagesize").unwrap_or(4096);
    let pages = sysctl_number(sysctl, "vm.stats.vm.v_free_count")?
        .saturating_add(sysctl_number(sysctl, "vm.stats.vm.v_inactive_count").unwrap_or(0));
    Some(Memory {
        total,
        available: pages.saturating_mul(page).min(total),
        swap_total: 0,
        swap_free: 0,
    })
}

/// macOS: installed memory, free, inactive and speculative pages (`vm_stat`), and swap.
fn mac_memory(lines: &[&str], sysctl: &HashMap<&str, &str>) -> Option<Memory> {
    if lines.is_empty() {
        return None;
    }
    let total = sysctl_number(sysctl, "hw.memsize")?;
    let page = lines
        .first()
        .and_then(|header| header.split("page size of ").nth(1))
        .and_then(|rest| numbers::<u64>(rest).first().copied())
        .or_else(|| sysctl_number(sysctl, "hw.pagesize"))
        .unwrap_or(4096);
    let pages = |name: &str| {
        lines
            .iter()
            .find_map(|line| line.strip_prefix(name))
            .and_then(|rest| {
                rest.trim_start_matches(':')
                    .trim()
                    .trim_end_matches('.')
                    .parse::<u64>()
                    .ok()
            })
            .unwrap_or(0)
    };
    let free = sum(["Pages free", "Pages inactive", "Pages speculative"].map(pages));
    let (swap_total, swap_free) = sysctl
        .get("vm.swapusage")
        .map(|usage| (swap_amount(usage, "total"), swap_amount(usage, "free")))
        .unwrap_or((0, 0));
    Some(Memory {
        total,
        available: free.saturating_mul(page).min(total),
        swap_total,
        swap_free,
    })
}

/// `name = 2048.00M` in `vm.swapusage`, in bytes.
fn swap_amount(usage: &str, name: &str) -> u64 {
    let Some(rest) = usage
        .split(&format!("{name} = "))
        .nth(1)
        .and_then(|rest| rest.split_whitespace().next())
    else {
        return 0;
    };
    // The unit is the last byte when it is an ASCII letter (a server's text may end in anything).
    let factor: f64 = match rest.as_bytes().last() {
        Some(b'K') => 1024.0,
        Some(b'M') => 1024.0 * 1024.0,
        Some(b'G') => 1024.0 * 1024.0 * 1024.0,
        _ => return rest.parse::<f64>().map_or(0, |bytes| bytes as u64),
    };
    rest.get(..rest.len() - 1)
        .unwrap_or_default()
        .parse::<f64>()
        .map_or(0, |value| (value * factor) as u64)
}

/// Interfaces never counted when there is no default route to go by.
fn is_virtual(name: &str) -> bool {
    [
        "lo", "docker", "veth", "br-", "virbr", "cni", "flannel", "cali", "tun", "tap", "wg",
        "utun",
    ]
    .iter()
    .any(|prefix| name.starts_with(prefix))
}

/// `/proc/net/dev`, counting the interface of the default route (`/proc/net/route`).
fn linux_traffic(lines: &[&str], route: &[&str]) -> Option<Traffic> {
    let defaults: Vec<&str> = route
        .iter()
        .filter_map(|line| {
            let fields: Vec<&str> = line.split_whitespace().collect();
            (fields.get(1) == Some(&"00000000") && fields.get(7) == Some(&"00000000"))
                .then(|| fields.first().copied())
                .flatten()
        })
        .collect();
    let mut total: Option<Traffic> = None;
    for line in lines {
        let Some((name, counters)) = line.split_once(':') else {
            continue;
        };
        let name = name.trim();
        if name.contains('|') {
            continue;
        }
        let counted = if defaults.is_empty() {
            !is_virtual(name)
        } else {
            defaults.contains(&name)
        };
        let values: Vec<u64> = numbers(counters);
        if let (true, Some(received), Some(sent)) = (counted, values.first(), values.get(8)) {
            let sum = total.get_or_insert(Traffic {
                received: 0,
                sent: 0,
            });
            sum.received = sum.received.saturating_add(*received);
            sum.sent = sum.sent.saturating_add(*sent);
        }
    }
    total
}

/// `netstat -ibn` (FreeBSD, macOS): the link-level rows (`<Link#n>`), counting the interface of
/// the default route (`route -n get default`). Columns are found from the header, from the
/// right: a row without an address has one field less on the left.
fn bsd_traffic(lines: &[&str], route: &[&str]) -> Option<Traffic> {
    let header: Vec<&str> = lines.first()?.split_whitespace().collect();
    let from_right = |column: &str| {
        header
            .iter()
            .position(|name| *name == column)
            .map(|index| header.len() - index)
    };
    let (received_at, sent_at) = (from_right("Ibytes")?, from_right("Obytes")?);
    let default = route.iter().find_map(|line| {
        line.trim()
            .strip_prefix("interface:")
            .map(|name| name.trim().to_owned())
    });
    let mut total: Option<Traffic> = None;
    for line in &lines[1..] {
        let fields: Vec<&str> = line.split_whitespace().collect();
        let (Some(name), Some(network)) = (fields.first(), fields.get(2)) else {
            continue;
        };
        if !network.starts_with("<Link") {
            continue;
        }
        let counted = match &default {
            Some(default) => name == default,
            None => !is_virtual(name),
        };
        let value = |from: usize| {
            fields
                .len()
                .checked_sub(from)
                .and_then(|index| fields.get(index))
                .and_then(|value| value.parse::<u64>().ok())
        };
        if let (true, Some(received), Some(sent)) = (counted, value(received_at), value(sent_at)) {
            let sum = total.get_or_insert(Traffic {
                received: 0,
                sent: 0,
            });
            sum.received = sum.received.saturating_add(received);
            sum.sent = sum.sent.saturating_add(sent);
        }
    }
    total
}

/// `kern.boottime` (`{ sec = 1790400000, usec = ... } ...`) and the time now.
fn bsd_uptime(sysctl: &HashMap<&str, &str>) -> Option<u64> {
    let boot = sysctl
        .get("kern.boottime")?
        .split("sec = ")
        .nth(1)
        .and_then(|rest| numbers::<u64>(&rest.replace(',', " ")).first().copied())?;
    let now = sysctl_number(sysctl, "now")?;
    now.checked_sub(boot)
}

/// File systems that aren't disks.
const PSEUDO_FILESYSTEMS: &[&str] = &[
    "none",
    "tmpfs",
    "devtmpfs",
    "udev",
    "devfs",
    "rootfs",
    "overlay",
    "shm",
    "proc",
    "sysfs",
    "cgroup",
    "cgroup2",
    "fdescfs",
    "procfs",
    "linprocfs",
    "squashfs",
    "efivarfs",
];

/// Mount points that aren't the user's disks.
const PSEUDO_MOUNTS: &[&str] = &[
    "/proc",
    "/sys",
    "/dev",
    "/run",
    "/snap/",
    "/System/Volumes/",
];

/// `df -Pk` (sizes in KiB): real file systems, each device once.
fn disks(lines: &[&str]) -> Vec<Disk> {
    let mut out: Vec<Disk> = Vec::new();
    for line in lines.iter().skip(1) {
        let fields: Vec<&str> = line.split_whitespace().collect();
        // The numbers start after the file system's name, which may have spaces (`map auto_home`).
        let Some(start) = (1..fields.len()).find(|&index| {
            fields.len() > index + 4
                && fields[index..index + 3]
                    .iter()
                    .all(|field| field.parse::<u64>().is_ok())
                && fields[index + 3].ends_with('%')
        }) else {
            continue;
        };
        let number = |index: usize| {
            fields[index]
                .parse::<u64>()
                .map_or(0, |kib| kib.saturating_mul(1024))
        };
        let disk = Disk {
            filesystem: fields[..start].join(" "),
            mount: fields[start + 4..].join(" "),
            size: number(start),
            used: number(start + 1),
            available: number(start + 2),
        };
        let root = disk.mount == "/";
        let pseudo = PSEUDO_FILESYSTEMS.contains(&disk.filesystem.as_str())
            || disk.filesystem.starts_with("map ")
            || disk.filesystem.starts_with("/dev/loop")
            || (PSEUDO_MOUNTS.iter().any(|prefix| {
                disk.mount == prefix.trim_end_matches('/') || disk.mount.starts_with(prefix)
            }) && disk.mount != "/System/Volumes/Data")
            || disk.size == 0;
        // A device mounted twice (a bind mount) counts once.
        let seen = disk.filesystem.starts_with("/dev/")
            && out.iter().any(|other| other.filesystem == disk.filesystem);
        if root || (!pseudo && !seen) {
            if root {
                out.retain(|other| other.filesystem != disk.filesystem || other.mount == "/");
            }
            out.push(disk);
        }
    }
    out
}

/// `who`: the user, the line, and where from (in parentheses) when it says.
fn users(lines: &[&str]) -> Vec<User> {
    lines
        .iter()
        .filter_map(|line| {
            let mut fields = line.split_whitespace();
            let name = fields.next()?.to_owned();
            let line_name = fields.next().unwrap_or_default().to_owned();
            let from = line
                .rsplit_once('(')
                .and_then(|(_, rest)| rest.split_once(')'))
                .map(|(from, _)| from.trim().to_owned())
                .unwrap_or_default();
            Some(User {
                name,
                line: line_name,
                from,
            })
        })
        .collect()
}

/// What the monitor reports.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Monitoring {
    /// A reading, with rates from the one before.
    Reading(Snapshot),
    /// The server can't be monitored: why, for people (a server's own error line at most).
    Unsupported(String),
}

/// The first line of what the command printed on stderr, or its exit status.
fn failure(errors: &[u8], status: Option<u32>) -> String {
    let text = String::from_utf8_lossy(errors);
    let line: String = text
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or_default()
        .chars()
        .take(200)
        .collect();
    match (line.is_empty(), status) {
        (false, _) => line,
        (true, Some(status)) => format!("the command ended with status {status}"),
        (true, None) => "the server gave no answer".to_owned(),
    }
}

/// Watches the server of `connection`: a reading every `interval`, reported as it arrives, until
/// the channel or the connection closes (or the future is dropped). A server that gives nothing
/// useful (no POSIX `sh`, an unknown system) is reported once as [`Monitoring::Unsupported`].
pub async fn watch(connection: &Connection, interval: Duration, report: impl Fn(Monitoring)) {
    let opened = async {
        let target = connection.target().map_err(|error| error.to_string())?;
        let channel = target
            .channel_open_session()
            .await
            .map_err(|error| error.to_string())?;
        channel
            .exec(true, command(interval))
            .await
            .map_err(|error| error.to_string())?;
        Ok::<_, String>(channel)
    };
    let mut channel = match opened.await {
        Ok(channel) => channel,
        Err(reason) => {
            report(Monitoring::Unsupported(reason));
            return;
        }
    };
    let started = Instant::now();
    let mut pending: Vec<u8> = Vec::new();
    let mut errors: Vec<u8> = Vec::new();
    let mut status = None;
    let mut rates = Rates::default();
    let mut readings: u64 = 0;
    loop {
        let message = if readings == 0 {
            let left = FIRST_READING.saturating_sub(started.elapsed());
            match tokio::time::timeout(left, channel.wait()).await {
                Ok(message) => message,
                Err(_) => {
                    report(Monitoring::Unsupported(
                        "the server gave no reading in time".to_owned(),
                    ));
                    return;
                }
            }
        } else {
            channel.wait().await
        };
        match message {
            Some(ChannelMsg::Data { data }) => {
                pending.extend_from_slice(&data);
                while let Some(end) = find(&pending, b"@end\n") {
                    let block: Vec<u8> = pending.drain(..end + 5).collect();
                    let reading = parse(&String::from_utf8_lossy(&block));
                    if reading.is_empty() {
                        if readings == 0 {
                            report(Monitoring::Unsupported(
                                "this system has nothing the monitor can read".to_owned(),
                            ));
                            return;
                        }
                        continue;
                    }
                    readings += 1;
                    report(Monitoring::Reading(rates.next(&reading, Instant::now())));
                }
                if pending.len() > MAX_PENDING {
                    report(Monitoring::Unsupported(
                        "the server's answer isn't a reading".to_owned(),
                    ));
                    return;
                }
            }
            Some(ChannelMsg::ExtendedData { data, .. }) => {
                if errors.len() < 4096 {
                    errors.extend_from_slice(&data);
                }
            }
            Some(ChannelMsg::ExitStatus { exit_status }) => status = Some(exit_status),
            Some(ChannelMsg::Close) | None => break,
            Some(_) => {}
        }
    }
    if readings == 0 {
        report(Monitoring::Unsupported(failure(&errors, status)));
    }
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

/// A network address of an interface.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Address {
    /// The interface (`eth0`, `em0`).
    pub interface: String,
    /// The address, with its prefix length when the system gives it (`10.0.0.5/24`).
    pub address: String,
}

/// What the Info tab shows about a host.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HostInfo {
    /// The system's name and version (`Debian GNU/Linux 13 (trixie)`, `macOS 15.0`).
    pub os_name: String,
    /// The kernel and its release (`Linux 6.12.48`).
    pub kernel: String,
    /// The machine (`x86_64`, `arm64`).
    pub architecture: String,
    /// The host name.
    pub hostname: String,
    /// How many CPUs.
    pub cpus: Option<u32>,
    /// Addresses, loopback and link-local ones left out.
    pub addresses: Vec<Address>,
    /// Memory, disks, users, uptime and load (no rates from one reading, except macOS's CPU).
    pub snapshot: Snapshot,
}

/// What [`info_command`] printed.
#[must_use]
pub fn parse_info(output: &str) -> HostInfo {
    let sections = sections(output);
    let section = |name: &str| sections.get(name).map(Vec::as_slice).unwrap_or_default();
    let first = |name: &str| {
        section(name)
            .iter()
            .map(|line| line.trim())
            .find(|line| !line.is_empty())
            .unwrap_or_default()
            .to_owned()
    };
    let release = section("release").iter().find_map(|line| {
        line.strip_prefix("PRETTY_NAME=")
            .map(|name| name.trim().trim_matches('"').to_owned())
    });
    let sw_vers = |key: &str| {
        section("swvers").iter().find_map(|line| {
            line.strip_prefix(key)
                .map(|value| value.trim_start_matches(':').trim().to_owned())
        })
    };
    let mac = sw_vers("ProductName").map(|name| {
        let version = sw_vers("ProductVersion").unwrap_or_default();
        format!("{name} {version}").trim().to_owned()
    });
    let uname = first("uname");
    let words: Vec<&str> = uname.split_whitespace().collect();
    let kernel = words.iter().take(2).copied().collect::<Vec<_>>().join(" ");
    let architecture = if words.len() > 2 {
        words.last().copied().unwrap_or_default().to_owned()
    } else {
        String::new()
    };
    HostInfo {
        os_name: release
            .or(mac)
            .unwrap_or_else(|| words.first().copied().unwrap_or_default().to_owned()),
        kernel,
        architecture,
        hostname: first("hostname"),
        cpus: first("ncpu").parse().ok(),
        addresses: addresses(section("addr")),
        snapshot: Rates::default().next(&parse(output), Instant::now()),
    }
}

/// Addresses from `ip -o addr show` or `ifconfig -a` (BSD, macOS, busybox), without loopback and
/// link-local ones.
fn addresses(lines: &[&str]) -> Vec<Address> {
    let mut out = Vec::new();
    let mut interface = String::new();
    for line in lines {
        let fields: Vec<&str> = line.split_whitespace().collect();
        // `ip -o`: `2: eth0    inet 10.0.0.5/24 brd ...`.
        if fields.first().is_some_and(|first| first.ends_with(':'))
            && fields.len() > 3
            && (fields[2] == "inet" || fields[2] == "inet6")
        {
            push_address(&mut out, fields[1], fields[3]);
            continue;
        }
        // `ifconfig`: an interface line starts at the left margin.
        if !line.starts_with(char::is_whitespace)
            && let Some(name) = fields.first()
        {
            name.trim_end_matches(':').clone_into(&mut interface);
        }
        if let Some(position) = fields
            .iter()
            .position(|field| *field == "inet" || *field == "inet6")
            && let Some(address) = fields.get(position + 1)
        {
            let address = address.trim_start_matches("addr:");
            // busybox's IPv6 lines: `inet6 addr: fe80::1/64 Scope:Link`.
            let address = if address.is_empty() {
                fields.get(position + 2).copied().unwrap_or_default()
            } else {
                address
            };
            push_address(&mut out, &interface, address);
        }
    }
    out
}

fn push_address(out: &mut Vec<Address>, interface: &str, address: &str) {
    // `fe80::1%em0` scopes an IPv6 address to its interface.
    let address = address.split('%').next().unwrap_or_default();
    let bare = address
        .split('/')
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase();
    let skipped = bare.is_empty()
        || bare.starts_with("127.")
        || bare == "::1"
        || bare.starts_with("fe80:")
        || interface.starts_with("lo");
    if !skipped
        && !out
            .iter()
            .any(|seen: &Address| seen.interface == interface && seen.address == address)
    {
        out.push(Address {
            interface: interface.to_owned(),
            address: address.to_owned(),
        });
    }
}

/// Reads the host info of `connection`'s server.
///
/// # Errors
///
/// Why it couldn't: the channel, the command, or nothing useful came back (for people).
pub async fn host_info(connection: &Connection) -> Result<HostInfo, String> {
    let run = async {
        let target = connection.target().map_err(|error| error.to_string())?;
        let mut channel = target
            .channel_open_session()
            .await
            .map_err(|error| error.to_string())?;
        channel
            .exec(true, info_command())
            .await
            .map_err(|error| error.to_string())?;
        let mut output = Vec::new();
        let mut errors = Vec::new();
        let mut status = None;
        while let Some(message) = channel.wait().await {
            match message {
                ChannelMsg::Data { data } => {
                    if output.len() < MAX_PENDING {
                        output.extend_from_slice(&data);
                    }
                }
                ChannelMsg::ExtendedData { data, .. } => {
                    if errors.len() < 4096 {
                        errors.extend_from_slice(&data);
                    }
                }
                ChannelMsg::ExitStatus { exit_status } => status = Some(exit_status),
                ChannelMsg::Close => break,
                _ => {}
            }
        }
        let text = String::from_utf8_lossy(&output);
        if find(&output, b"@end").is_none() {
            return Err(failure(&errors, status));
        }
        Ok(parse_info(&text))
    };
    tokio::time::timeout(INFO_TIMEOUT, run)
        .await
        .unwrap_or_else(|_| Err("the server gave no answer in time".to_owned()))
}

#[cfg(test)]
mod tests;
