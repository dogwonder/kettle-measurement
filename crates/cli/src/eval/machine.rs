//! What machine produced these numbers (#25).
//!
//! An eval report is a claim about a model *on particular hardware* —
//! "0.96 end-to-end" means nothing without "on an M1 Pro with 16GB"
//! (`runner::eval::MachineInfo`). So the harness records the machine
//! rather than asking the person to remember to.
//!
//! Detection is best-effort and never fatal: a field that can't be read
//! says so. An eval that refused to run because it couldn't name the
//! CPU would be trading the measurement for its label.

use runner::eval::MachineInfo;
use std::process::Command;

/// What is recorded when the machine won't say.
const UNKNOWN: &str = "unknown";

/// The machine this eval is running on, as far as it can be determined.
pub fn detect() -> MachineInfo {
    MachineInfo {
        cpu: cpu().unwrap_or_else(|| UNKNOWN.to_owned()),
        ram_gb: ram_gb().unwrap_or(0),
        os: os().unwrap_or_else(|| UNKNOWN.to_owned()),
    }
}

#[cfg(target_os = "macos")]
fn cpu() -> Option<String> {
    sysctl("machdep.cpu.brand_string")
}

#[cfg(target_os = "macos")]
fn ram_gb() -> Option<u32> {
    let bytes: u64 = sysctl("hw.memsize")?.parse().ok()?;
    Some((bytes / 1024 / 1024 / 1024) as u32)
}

#[cfg(target_os = "macos")]
fn os() -> Option<String> {
    let version = run("sw_vers", &["-productVersion"])?;
    Some(format!("macOS {version}"))
}

#[cfg(target_os = "macos")]
fn sysctl(key: &str) -> Option<String> {
    run("sysctl", &["-n", key])
}

#[cfg(not(target_os = "macos"))]
fn cpu() -> Option<String> {
    let device_tree = std::fs::read_to_string("/proc/device-tree/model").ok();
    let cpuinfo = std::fs::read_to_string("/proc/cpuinfo").unwrap_or_default();
    cpu_from(device_tree.as_deref(), &cpuinfo)
}

#[cfg(not(target_os = "macos"))]
fn ram_gb() -> Option<u32> {
    ram_gb_from(&std::fs::read_to_string("/proc/meminfo").ok()?)
}

#[cfg(not(target_os = "macos"))]
fn os() -> Option<String> {
    std::fs::read_to_string("/etc/os-release")
        .ok()
        .and_then(|release| os_from(&release))
        .or_else(|| run("uname", &["-sr"]))
}

/// The machine's name, board first (#97).
///
/// `/proc/device-tree/model` is the only place a Raspberry Pi says it is
/// one — aarch64 `/proc/cpuinfo` has no `model name` line at all, so a
/// cpuinfo-only reading records nothing on exactly the machine this was
/// written for. The device tree string is NUL-terminated, being read
/// straight out of the firmware blob.
#[cfg_attr(target_os = "macos", allow(dead_code))]
fn cpu_from(device_tree_model: Option<&str>, cpuinfo: &str) -> Option<String> {
    if let Some(model) = device_tree_model {
        let model = model.trim_end_matches('\0').trim();
        if !model.is_empty() {
            return Some(model.to_owned());
        }
    }
    // x86 spells it `model name`; aarch64 kernels put the board under
    // `Model`. Try both rather than guessing the architecture.
    ["model name", "Model"].into_iter().find_map(|key| {
        cpuinfo.lines().find_map(|line| {
            let (name, value) = line.split_once(':')?;
            (name.trim() == key).then(|| value.trim().to_owned())
        })
    })
}

/// Installed memory, rounded to the nearest gibibyte.
///
/// Rounded, not truncated: `MemTotal` excludes memory the firmware has
/// reserved, so an 8GB Pi reports about 7.86GiB and truncation would
/// record 7. That number is not trivia — it decides which tier sentence
/// a person is shown about their own machine (#39).
#[cfg_attr(target_os = "macos", allow(dead_code))]
fn ram_gb_from(meminfo: &str) -> Option<u32> {
    let kib: u64 = meminfo.lines().find_map(|line| {
        let (name, value) = line.split_once(':')?;
        (name.trim() == "MemTotal").then(|| value.split_whitespace().next()?.parse().ok())?
    })?;
    Some(((kib + 512 * 1024) / (1024 * 1024)) as u32)
}

/// What the distribution calls itself, e.g. "Debian GNU/Linux 12
/// (bookworm)" — the name its users would recognise, rather than
/// `uname`'s kernel version.
#[cfg_attr(target_os = "macos", allow(dead_code))]
fn os_from(os_release: &str) -> Option<String> {
    os_release.lines().find_map(|line| {
        let value = line.strip_prefix("PRETTY_NAME=")?;
        Some(value.trim().trim_matches('"').to_owned())
    })
}

/// A command's trimmed stdout, or `None` if it isn't there or fails —
/// the machine simply declines to say.
fn run(binary: &str, args: &[&str]) -> Option<String> {
    let output = Command::new(binary).args(args).output().ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8(output.stdout).ok()?.trim().to_owned();
    if text.is_empty() {
        None
    } else {
        Some(text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A Pi 5's `/proc/cpuinfo`. There is no `model name` line on
    /// aarch64 — the useful name is the board's, which is why the
    /// device tree is asked first.
    const PI5_CPUINFO: &str = "\
processor\t: 0
BogoMIPS\t: 108.00
CPU implementer\t: 0x41
CPU architecture: 8
CPU part\t: 0xd0b
CPU revision\t: 1

Model\t\t: Raspberry Pi 5 Model B Rev 1.0
";

    const X86_CPUINFO: &str = "\
processor\t: 0
vendor_id\t: GenuineIntel
model name\t: Intel(R) Core(TM) i7-8700B CPU @ 3.20GHz
cpu MHz\t\t: 3200.000
";

    /// #97: a Pi measurement is a claim about hardware, so recording
    /// `unknown` for the hardware would leave the claim unmade. The
    /// board name is what identifies a Pi, and it lives in the device
    /// tree rather than in cpuinfo.
    #[test]
    fn a_pi_is_named_by_its_board() {
        assert_eq!(
            cpu_from(Some("Raspberry Pi 5 Model B Rev 1.0\0"), PI5_CPUINFO).as_deref(),
            Some("Raspberry Pi 5 Model B Rev 1.0"),
            "the device tree name wins, with its trailing NUL trimmed",
        );
    }

    /// Without a device tree — an ordinary Linux box, a VM — cpuinfo
    /// still answers, by whichever key that architecture uses.
    #[test]
    fn other_linux_machines_fall_back_to_cpuinfo() {
        assert_eq!(
            cpu_from(None, X86_CPUINFO).as_deref(),
            Some("Intel(R) Core(TM) i7-8700B CPU @ 3.20GHz"),
        );
        assert_eq!(
            cpu_from(None, PI5_CPUINFO).as_deref(),
            Some("Raspberry Pi 5 Model B Rev 1.0"),
            "aarch64 cpuinfo spells it Model, not model name",
        );
        assert_eq!(cpu_from(None, "processor\t: 0\n"), None);
    }

    /// An 8GB Pi reports about 7.86GiB, because firmware reserves the
    /// rest — truncating would record 7GB and put the machine in the
    /// wrong tier, which is a sentence shown to a person about their
    /// own computer.
    #[test]
    fn installed_memory_rounds_to_what_was_bought() {
        assert_eq!(ram_gb_from("MemTotal:        8244480 kB\n"), Some(8));
        assert_eq!(ram_gb_from("MemTotal:        4050000 kB\n"), Some(4));
        assert_eq!(ram_gb_from("MemTotal:       16307840 kB\n"), Some(16));
        assert_eq!(ram_gb_from("SwapTotal:  0 kB\n"), None);
    }

    #[test]
    fn the_os_is_named_the_way_its_users_name_it() {
        assert_eq!(
            os_from("PRETTY_NAME=\"Debian GNU/Linux 12 (bookworm)\"\nID=debian\n").as_deref(),
            Some("Debian GNU/Linux 12 (bookworm)"),
        );
        assert_eq!(os_from("ID=debian\n"), None);
    }
}
