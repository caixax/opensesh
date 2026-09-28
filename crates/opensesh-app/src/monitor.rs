//! The remote monitor's readings and the host info, as the JSON the status bar and the side
//! panel's Info tab read (`TerminalItem.monitor` and `TerminalItem.hostInfo`).

use opensesh_ssh::monitor::{Disk, HostInfo, Monitoring, Snapshot, User};
use serde_json::{Value as Json, json};

use crate::terminal::registry::HostInfoState;

fn disk_json(disk: &Disk) -> Json {
    json!({
        "filesystem": disk.filesystem,
        "mount": disk.mount,
        "size": disk.size,
        "used": disk.used,
        "available": disk.available,
    })
}

fn user_json(user: &User) -> Json {
    json!({ "name": user.name, "line": user.line, "from": user.from })
}

/// A snapshot: CPU in percent (one decimal), memory and disks in bytes, network in bytes per
/// second, uptime in seconds, the load average; `null` for what the server didn't say.
fn snapshot_json(snapshot: &Snapshot) -> Json {
    json!({
        "cpu": snapshot.cpu_permille.map(|permille| f64::from(permille) / 10.0),
        "memory": snapshot.memory.map(|memory| json!({
            "total": memory.total,
            "available": memory.available,
            "swapTotal": memory.swap_total,
            "swapFree": memory.swap_free,
        })),
        "received": snapshot.received_per_sec,
        "sent": snapshot.sent_per_sec,
        "disks": snapshot.disks.iter().map(disk_json).collect::<Vec<_>>(),
        "root": snapshot.root_disk().map(disk_json),
        "uptime": snapshot.uptime_secs,
        "load": snapshot.load_hundredths.map(|load| load.map(|value| f64::from(value) / 100.0)),
        "users": snapshot.users.iter().map(user_json).collect::<Vec<_>>(),
    })
}

/// `{state: "reading", ...snapshot}` or `{state: "unsupported", reason}`.
#[must_use]
pub fn monitoring_json(monitoring: &Monitoring) -> String {
    match monitoring {
        Monitoring::Reading(snapshot) => {
            let mut value = snapshot_json(snapshot);
            if let Json::Object(map) = &mut value {
                map.insert("state".into(), Json::from("reading"));
            }
            value.to_string()
        }
        Monitoring::Unsupported(reason) => {
            json!({ "state": "unsupported", "reason": reason }).to_string()
        }
    }
}

fn info_json(info: &HostInfo) -> Json {
    json!({
        "state": "ready",
        "osName": info.os_name,
        "kernel": info.kernel,
        "architecture": info.architecture,
        "hostname": info.hostname,
        "cpus": info.cpus,
        "addresses": info.addresses.iter().map(|address| json!({
            "interface": address.interface,
            "address": address.address,
        })).collect::<Vec<_>>(),
        "snapshot": snapshot_json(&info.snapshot),
    })
}

/// `{state: "reading"}`, `{state: "ready", osName, kernel, ...}` or `{state: "failed", reason}`.
#[must_use]
pub fn host_info_json(state: &HostInfoState) -> String {
    match state {
        HostInfoState::Reading => json!({ "state": "reading" }).to_string(),
        HostInfoState::Ready(info) => info_json(info).to_string(),
        HostInfoState::Failed(reason) => json!({ "state": "failed", "reason": reason }).to_string(),
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, reason = "tests")]

    use opensesh_ssh::monitor::{Memory, Rates, parse, parse_info};

    use super::*;

    #[test]
    fn a_reading_as_json() {
        let snapshot = Snapshot {
            cpu_permille: Some(123),
            memory: Some(Memory {
                total: 8,
                available: 5,
                swap_total: 2,
                swap_free: 1,
            }),
            received_per_sec: Some(1_250_000),
            sent_per_sec: None,
            load_hundredths: Some([12, 25, 150]),
            ..Snapshot::default()
        };
        let value: Json =
            serde_json::from_str(&monitoring_json(&Monitoring::Reading(snapshot))).unwrap();
        assert_eq!(value["state"], "reading");
        assert_eq!(value["cpu"], 12.3);
        assert_eq!(value["memory"]["swapFree"], 1);
        assert_eq!(value["received"], 1_250_000);
        assert!(value["sent"].is_null() && value["root"].is_null() && value["uptime"].is_null());
        assert_eq!(value["load"], json!([0.12, 0.25, 1.5]));
        let unsupported: Json = serde_json::from_str(&monitoring_json(&Monitoring::Unsupported(
            "sh: not found".into(),
        )))
        .unwrap();
        assert_eq!(
            unsupported,
            json!({ "state": "unsupported", "reason": "sh: not found" })
        );
    }

    #[test]
    fn the_host_info_as_json() {
        let output = "@release\nPRETTY_NAME=\"Debian GNU/Linux 13 (trixie)\"\n@uname\nLinux 6.12 x86_64\n\
                      @hostname\nweb-01\n@ncpu\n4\n@addr\n2: eth0 inet 10.0.0.5/24 scope global eth0\n\
                      @os Linux\n@df\nFilesystem 1024-blocks Used Available Capacity Mounted on\n\
                      /dev/vda1 100 40 60 40% /\n@who\ndeploy pts/0 2026-09-28 09:00 (10.0.0.2)\n@end\n";
        let value: Json = serde_json::from_str(&host_info_json(&HostInfoState::Ready(Box::new(
            parse_info(output),
        ))))
        .unwrap();
        assert_eq!(value["state"], "ready");
        assert_eq!(value["osName"], "Debian GNU/Linux 13 (trixie)");
        assert_eq!(value["cpus"], 4);
        assert_eq!(value["addresses"][0]["address"], "10.0.0.5/24");
        assert_eq!(value["snapshot"]["root"]["used"], 40 * 1024);
        assert_eq!(value["snapshot"]["users"][0]["from"], "10.0.0.2");
        let reading: Json = serde_json::from_str(&host_info_json(&HostInfoState::Reading)).unwrap();
        assert_eq!(reading["state"], "reading");
        // A reading's snapshot has the same shape.
        let snapshot = Rates::default().next(&parse(output), std::time::Instant::now());
        assert_eq!(snapshot_json(&snapshot), value["snapshot"]);
    }
}
