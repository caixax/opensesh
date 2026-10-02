#![allow(clippy::unwrap_used, reason = "tests")]

use super::*;

const DEBIAN: &str = include_str!("fixtures/debian.txt");
const ARCH: &str = include_str!("fixtures/arch.txt");
const BUSYBOX: &str = include_str!("fixtures/busybox.txt");
const FREEBSD: &str = include_str!("fixtures/freebsd.txt");
const MACOS: &str = include_str!("fixtures/macos.txt");
const INFO_DEBIAN: &str = include_str!("fixtures/info-debian.txt");
const INFO_BUSYBOX: &str = include_str!("fixtures/info-busybox.txt");

const KIB: u64 = 1024;
const MIB: u64 = 1024 * 1024;

fn mounts(reading: &Reading) -> Vec<&str> {
    reading
        .disks
        .iter()
        .map(|disk| disk.mount.as_str())
        .collect()
}

#[test]
fn the_commands_survive_any_login_shell() {
    for command in [command(Duration::from_secs(3)), info_command()] {
        let script = command
            .strip_prefix("sh -c '")
            .and_then(|rest| rest.strip_suffix('\''))
            .unwrap();
        // One line, no single quote to end the quoting, no `!` for csh's history.
        assert!(!script.contains('\''), "{script}");
        assert!(!script.contains('!'), "{script}");
        assert!(!script.contains('\n'), "{script}");
        assert!(script.contains("echo @end"));
    }
    assert!(command(Duration::from_secs(3)).contains("n=3;"));
    assert!(command(Duration::ZERO).contains("n=1;"));
    assert!(command(Duration::from_secs(86_400)).contains("n=3600;"));
}

#[test]
fn a_debian_reading() {
    let reading = parse(DEBIAN);
    assert_eq!(reading.os, "Linux");
    // cpu  108 0 176 5364 154 0 42 0 0 0: idle is idle and iowait.
    assert_eq!(
        reading.cpu,
        Some(CpuTicks {
            busy: 326,
            total: 5844
        })
    );
    assert_eq!(reading.cpu_percent, None);
    assert_eq!(
        reading.memory,
        Some(Memory {
            total: 16_278_176 * KIB,
            available: 15_552_068 * KIB,
            swap_total: 4_194_304 * KIB,
            swap_free: 4_194_304 * KIB,
        })
    );
    // eth0, the default route's interface; lo isn't counted.
    assert_eq!(
        reading.traffic,
        Some(Traffic {
            received: 1820,
            sent: 606
        })
    );
    assert_eq!(reading.uptime, Some(3));
    assert_eq!(reading.load, Some([0.0, 0.0, 0.0]));
    // WSL's tmpfs, `none` and `rootfs` mounts are left out.
    assert_eq!(mounts(&reading), ["/usr/lib/wsl/drivers", "/"]);
    let root = reading.disks.iter().find(|disk| disk.mount == "/").unwrap();
    assert_eq!(root.filesystem, "/dev/sdd");
    assert_eq!(root.size, 1_055_762_868 * KIB);
    assert_eq!(root.used, 64_485_108 * KIB);
    assert_eq!(root.available, 937_574_288 * KIB);
    assert_eq!(
        reading.users,
        [User {
            name: "deploy".into(),
            line: "pts/1".into(),
            from: String::new()
        }]
    );
}

#[test]
fn an_arch_reading() {
    let reading = parse(ARCH);
    assert!(reading.cpu.is_some() && reading.memory.is_some() && reading.traffic.is_some());
    assert!(mounts(&reading).contains(&"/"));
    assert_eq!(
        reading.users.first().map(|user| user.name.as_str()),
        Some("root")
    );
}

#[test]
fn a_busybox_reading() {
    let reading = parse(BUSYBOX);
    assert_eq!(
        reading.cpu,
        Some(CpuTicks {
            busy: 1239,
            total: 24_114
        })
    );
    assert_eq!(reading.memory.unwrap().available, 15_220_332 * KIB);
    assert_eq!(
        reading.traffic,
        Some(Traffic {
            received: 9188,
            sent: 6916
        })
    );
    assert_eq!(reading.uptime, Some(13));
    assert_eq!(reading.load, Some([0.15, 0.03, 0.01]));
    // The same device mounted a second time counts once.
    assert_eq!(mounts(&reading), ["/usr/lib/wsl/drivers", "/"]);
    // busybox's `who` has an idle column and no host.
    assert_eq!(reading.users.len(), 1);
    assert_eq!(reading.users[0].line, "pts/1");
    assert_eq!(reading.users[0].from, "");
}

#[test]
fn a_freebsd_reading() {
    let reading = parse(FREEBSD);
    assert_eq!(reading.os, "FreeBSD");
    // kern.cp_time 15230 12 8410 1320 982340.
    assert_eq!(
        reading.cpu,
        Some(CpuTicks {
            busy: 24_972,
            total: 1_007_312
        })
    );
    assert_eq!(
        reading.memory,
        Some(Memory {
            total: 8_540_577_792,
            available: (1_523_456 + 204_800) * 4096,
            swap_total: 0,
            swap_free: 0,
        })
    );
    // vtnet0's link-level row only.
    assert_eq!(
        reading.traffic,
        Some(Traffic {
            received: 1_523_456_789,
            sent: 123_456_789
        })
    );
    assert_eq!(reading.uptime, Some(103_872));
    assert_eq!(reading.load, Some([0.21, 0.30, 0.28]));
    assert_eq!(mounts(&reading), ["/", "/usr/home", "/var/log"]);
    assert_eq!(reading.disks[0].filesystem, "zroot/ROOT/default");
    assert_eq!(reading.users[0].from, "10.0.0.2");
}

#[test]
fn a_macos_reading() {
    let reading = parse(MACOS);
    assert_eq!(reading.os, "Darwin");
    assert_eq!(reading.cpu, None);
    // (12.5 + 3.5 + 0 + 40) over 8 CPUs.
    assert_eq!(reading.cpu_percent, Some(7.0));
    assert_eq!(
        reading.memory,
        Some(Memory {
            total: 17_179_869_184,
            available: (12_345 + 234_567 + 4567) * 16_384,
            swap_total: 2048 * MIB,
            swap_free: 1_610_350_592,
        })
    );
    // en0's link row, found from the right: lo0's row has no address.
    assert_eq!(
        reading.traffic,
        Some(Traffic {
            received: 12_345_678_901,
            sent: 987_654_321
        })
    );
    assert_eq!(reading.uptime, Some(103_872));
    assert_eq!(reading.load, Some([1.62, 1.80, 1.95]));
    assert_eq!(mounts(&reading), ["/", "/System/Volumes/Data"]);
    assert_eq!(reading.users.len(), 2);
    assert_eq!(reading.users[1].from, "10.0.0.2");
}

#[test]
fn without_a_default_route_every_real_interface_counts() {
    let output = "@os Linux\n@netdev\nInter-| Receive\n face |bytes\nlo: 5 0 0 0 0 0 0 0 5 0 0 0 0 0 0 0\n\
                  eth0:100 1 0 0 0 0 0 0 10 1 0 0 0 0 0 0\nwlan0: 200 1 0 0 0 0 0 0 20 1 0 0 0 0 0 0\n\
                  docker0: 999 1 0 0 0 0 0 0 999 1 0 0 0 0 0 0\n@end\n";
    assert_eq!(
        parse(output).traffic,
        Some(Traffic {
            received: 300,
            sent: 30
        })
    );
}

#[test]
fn nothing_readable_is_empty() {
    assert!(parse("").is_empty());
    assert!(parse("'sh' is not recognized as an internal or external command\n").is_empty());
    assert!(parse("@os Plan9\n@df\n@who\n@end\n").is_empty());
    assert!(!parse(DEBIAN).is_empty());
}

#[test]
fn rates_come_from_two_readings() {
    let mut rates = Rates::default();
    let start = Instant::now();
    let reading = |busy, total, received, sent| Reading {
        cpu: Some(CpuTicks { busy, total }),
        traffic: Some(Traffic { received, sent }),
        ..Reading::default()
    };
    let first = rates.next(&reading(100, 1000, 10_000, 2000), start);
    assert_eq!(first.cpu_permille, None);
    assert_eq!(first.received_per_sec, None);
    let second = rates.next(
        &reading(350, 2000, 40_000, 5000),
        start + Duration::from_secs(3),
    );
    // 250 busy ticks of 1000: 25.0 %.
    assert_eq!(second.cpu_permille, Some(250));
    assert_eq!(second.received_per_sec, Some(10_000));
    assert_eq!(second.sent_per_sec, Some(1000));
    // Counters that went back (a reboot, a new interface) give no rate.
    let third = rates.next(&reading(10, 100, 5, 5), start + Duration::from_secs(6));
    assert_eq!(third.cpu_permille, None);
    assert_eq!(third.received_per_sec, None);
    // macOS gives a percentage directly.
    let mac = Rates::default().next(&parse(MACOS), start);
    assert_eq!(mac.cpu_permille, Some(70));
    assert_eq!(mac.load_hundredths, Some([162, 180, 195]));
    assert_eq!(mac.root_disk().map(|disk| disk.mount.as_str()), Some("/"));
}

#[test]
fn the_host_info_of_debian_and_busybox() {
    let debian = parse_info(INFO_DEBIAN);
    assert_eq!(debian.os_name, "Debian GNU/Linux 13 (trixie)");
    assert_eq!(debian.kernel, "Linux 6.18.33.2-microsoft-standard-WSL2");
    assert_eq!(debian.architecture, "x86_64");
    assert_eq!(debian.hostname, "web-01");
    assert_eq!(debian.cpus, Some(20));
    // Loopback and link-local addresses are left out.
    assert_eq!(
        debian.addresses,
        [Address {
            interface: "eth0".into(),
            address: "10.0.0.5/24".into()
        }]
    );
    assert_eq!(debian.snapshot.memory.unwrap().total, 16_278_176 * KIB);
    assert_eq!(debian.snapshot.root_disk().unwrap().mount, "/");
    let busybox = parse_info(INFO_BUSYBOX);
    assert_eq!(busybox.os_name, "Ubuntu 22.04.5 LTS");
    assert_eq!(busybox.addresses, debian.addresses);
    assert_eq!(busybox.snapshot.users.len(), 1);
}

#[test]
fn addresses_from_ifconfig() {
    let freebsd = "vtnet0: flags=8843<UP,BROADCAST,RUNNING,SIMPLEX,MULTICAST> metric 0 mtu 1500\n\
                   \tether 58:9c:fc:00:12:34\n\
                   \tinet 10.0.0.5 netmask 0xffffff00 broadcast 10.0.0.255\n\
                   \tinet6 fe80::5a9c:fcff:fe00:1234%vtnet0 prefixlen 64 scopeid 0x1\n\
                   \tinet6 2001:db8::5 prefixlen 64\n\
                   lo0: flags=8049<UP,LOOPBACK,RUNNING,MULTICAST> metric 0 mtu 16384\n\
                   \tinet 127.0.0.1 netmask 0xff000000\n";
    let busybox = "eth0      Link encap:Ethernet  HWaddr 52:54:00:12:34:56\n\
                   \x20         inet addr:10.0.0.7  Bcast:10.0.0.255  Mask:255.255.255.0\n\
                   \x20         inet6 addr: 2001:db8::7/64 Scope:Global\n\
                   \x20         inet6 addr: fe80::5054:ff:fe12:3456/64 Scope:Link\n";
    let address = |interface: &str, address: &str| Address {
        interface: interface.into(),
        address: address.into(),
    };
    let lines = |text: &'static str| text.lines().collect::<Vec<_>>();
    assert_eq!(
        addresses(&lines(freebsd)),
        [
            address("vtnet0", "10.0.0.5"),
            address("vtnet0", "2001:db8::5")
        ]
    );
    assert_eq!(
        addresses(&lines(busybox)),
        [
            address("eth0", "10.0.0.7"),
            address("eth0", "2001:db8::7/64")
        ]
    );
}

#[test]
fn macos_names_itself() {
    let info = parse_info(
        "@uname\nDarwin 24.0.0 arm64\n@hostname\nstudio.local\n@ncpu\n8\n\
         @swvers\nProductName:\t\tmacOS\nProductVersion:\t\t15.0\nBuildVersion:\t\t24A335\n",
    );
    assert_eq!(info.os_name, "macOS 15.0");
    assert_eq!(info.kernel, "Darwin 24.0.0");
    assert_eq!(info.architecture, "arm64");
    assert_eq!(info.cpus, Some(8));
}

#[test]
fn failures_say_why_in_one_line() {
    assert_eq!(
        failure(
            b"\n'sh' is not recognized as an internal or external command,\r\noperable program\r\n",
            Some(1)
        ),
        "'sh' is not recognized as an internal or external command,"
    );
    assert_eq!(failure(b"", Some(127)), "the command ended with status 127");
    assert_eq!(failure(b"", None), "the server gave no answer");
}

#[test]
fn a_unit_after_a_multibyte_character_is_no_crash() {
    // Found by fuzzing (Sprint 17): `vm.swapusage` whose number ends in a character of several
    // bytes, which `parse` sees after the lossy decoding of the server's output.
    let output = String::from_utf8_lossy(include_bytes!("fixtures/fuzz-swapusage.bin"));
    let _ = parse(&output);
    assert_eq!(swap_amount("total = 1535.7\u{fffd}", "total"), 0);
    assert_eq!(
        swap_amount("total = 2048.00M  used", "total"),
        2048 * 1024 * 1024
    );
}
