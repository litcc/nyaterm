use std::collections::HashSet;
use std::time::Duration;

use serde::Serialize;

use crate::{
    RemoteCommandOutput, SshMultiplexHandle, SshSessionConfig, ensure_remote_command_success,
    run_ssh_command,
};

const STATS_TIMEOUT: Duration = Duration::from_secs(15);

#[derive(Debug, Clone, Serialize, Default, PartialEq)]
pub struct SystemInfo {
    pub hostname: String,
    pub uptime_sec: u64,
    pub os: String,
    pub arch: String,
}

#[derive(Debug, Clone, Serialize, Default, PartialEq)]
pub struct LoadInfo {
    pub load1: f64,
    pub load5: f64,
    pub load15: f64,
}

#[derive(Debug, Clone, Serialize, Default, PartialEq)]
pub struct CpuInfo {
    pub model: String,
    pub cores: u32,
    pub usage: f64,
    pub per_core: Vec<f64>,
}

#[derive(Debug, Clone, Serialize, Default, PartialEq)]
pub struct MemoryInfo {
    pub used: u64,
    pub available: u64,
    pub cached: u64,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct NetworkInfo {
    pub nic: String,
    pub state: String,
    pub rx_bytes_per_sec: f64,
    pub tx_bytes_per_sec: f64,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct DiskInfo {
    pub device: String,
    pub mount: String,
    pub total: u64,
    pub available: u64,
    pub use_percent: u32,
}

#[derive(Debug, Clone, Serialize, Default, PartialEq)]
pub struct RemoteStats {
    pub system: SystemInfo,
    pub load: LoadInfo,
    pub cpu: CpuInfo,
    pub memory: MemoryInfo,
    pub networks: Vec<NetworkInfo>,
    pub disks: Vec<DiskInfo>,
}

#[derive(Debug, Clone)]
pub struct RemoteStatsService {
    config: SshSessionConfig,
    multiplex: Option<SshMultiplexHandle>,
}

pub const SYSINFO_SCRIPT: &str = r#"sh -c '
base=${TMPDIR:-/tmp}/sysinfo.$$;
cpu1=$base.cpu1;
cpu2=$base.cpu2;
net1=$base.net1;
net2=$base.net2;
netr=$base.netr;
diskf=$base.disk;

trap "rm -f \"$cpu1\" \"$cpu2\" \"$net1\" \"$net2\" \"$netr\" \"$diskf\"" 0 HUP INT TERM;

host=$(cat /proc/sys/kernel/hostname 2>/dev/null);
[ -n "$host" ] || host=$(uname -n);
host=$(printf "%s" "$host" | tr "\t\r\n" "   ");

read upraw _ </proc/uptime;
uptime_sec=${upraw%.*};

if [ -r /etc/os-release ]; then
  . /etc/os-release;
  os=${PRETTY_NAME:-unknown};
else
  os=$(uname -s);
fi;
os=$(printf "%s" "$os" | tr "\t\r\n" "   ");

arch=$(uname -m);

read l1 l5 l15 _ </proc/loadavg;

cpu_model=$(awk -F: '"'"'
/^(model name|Hardware|Processor|cpu model)[[:space:]]*:/ && !m {
  gsub(/^[ \t]+/, "", $2);
  m=$2;
}
END {
  if (!m) m="unknown";
  print m;
}
'"'"' /proc/cpuinfo 2>/dev/null);

cpu_model=$(printf "%s" "$cpu_model" | tr "\t\r\n" "   ");

cpu_cores=$(awk '"'"'
/^processor[[:space:]]*:/ { c++ }
END { print c+0 }
'"'"' /proc/cpuinfo 2>/dev/null);

case $cpu_cores in
  ""|0) cpu_cores=$(getconf _NPROCESSORS_ONLN 2>/dev/null || echo 0) ;;
esac;

awk '"'"'
/^cpu/ {
  idle=$5+$6;
  total=0;
  for (i=2; i<=NF; i++) total+=$i;
  print $1, idle, total;
}
'"'"' /proc/stat >"$cpu1";

awk '"'"'
NR>2 {
  line=$0;
  sub(/^[ \t]+/, "", line);
  split(line, a, ":");
  nic=a[1];
  gsub(/^[ \t]+|[ \t]+$/, "", nic);
  gsub(/^[ \t]+/, "", a[2]);
  split(a[2], f, /[ \t]+/);
  print nic "\t" f[1] "\t" f[9];
}
'"'"' /proc/net/dev >"$net1";

interval=0.2;
sleep "$interval" 2>/dev/null || {
  interval=1;
  sleep 1;
};

awk '"'"'
/^cpu/ {
  idle=$5+$6;
  total=0;
  for (i=2; i<=NF; i++) total+=$i;
  print $1, idle, total;
}
'"'"' /proc/stat >"$cpu2";

awk '"'"'
NR>2 {
  line=$0;
  sub(/^[ \t]+/, "", line);
  split(line, a, ":");
  nic=a[1];
  gsub(/^[ \t]+|[ \t]+$/, "", nic);
  gsub(/^[ \t]+/, "", a[2]);
  split(a[2], f, /[ \t]+/);
  print nic "\t" f[1] "\t" f[9];
}
'"'"' /proc/net/dev >"$net2";

cpu_usage=$(awk '"'"'
NR==FNR {
  id[$1]=$2;
  tot[$1]=$3;
  next;
}
$1=="cpu" {
  didle=$2-id[$1];
  dtotal=$3-tot[$1];
  cpu=(dtotal>0) ? (1-didle/dtotal)*100 : 0;
  printf "%.1f", cpu;
}
'"'"' "$cpu1" "$cpu2");

set -- $(awk '"'"'
/MemTotal:/ { t=$2 }
/MemAvailable:/ { a=$2 }
/Buffers:/ { b=$2 }
/^Cached:/ { c=$2 }
/SReclaimable:/ { s=$2 }
END {
  printf "%.0f %.0f %.0f\n", (t-a)*1024, a*1024, (b+c+s)*1024;
}
'"'"' /proc/meminfo);

mem_used=$1;
mem_avail=$2;
mem_cache=$3;

printf "SYSTEM\t%s\t%s\t%s\t%s\n" "$host" "$uptime_sec" "$os" "$arch";
printf "LOAD\t%s\t%s\t%s\n" "$l1" "$l5" "$l15";
printf "CPU\t%s\t%s\t%s\n" "$cpu_model" "$cpu_cores" "$cpu_usage";

awk '"'"'
NR==FNR {
  id[$1]=$2;
  tot[$1]=$3;
  next;
}
/^cpu[0-9]/ {
  didle=$2-id[$1];
  dtotal=$3-tot[$1];
  cpu=(dtotal>0) ? (1-didle/dtotal)*100 : 0;
  n=substr($1,4);
  printf "CPUCORE\t%s\t%.1f\n", n, cpu;
}
'"'"' "$cpu1" "$cpu2";

printf "MEMORY\t%s\t%s\t%s\n" "$mem_used" "$mem_avail" "$mem_cache";

awk -v s="$interval" '"'"'
BEGIN {
  OFS="\t";
}
FNR==NR {
  rx[$1]=$2;
  tx[$1]=$3;
  next;
}
{
  nic=$1;

  if (nic=="" || nic=="lo") next;
  if (nic ~ /^(docker|veth|br-|virbr|flannel|cali|tunl|kube-ipvs0|cni|zt|tailscale|wg|tap|vnet)/) next;

  rxv=($2-rx[nic])/s;
  txv=($3-tx[nic])/s;

  if (rxv<0) rxv=0;
  if (txv<0) txv=0;

  printf "%s\t%.0f\t%.0f\n", nic, rxv, txv;
}
'"'"' "$net1" "$net2" >"$netr";

found_net=0;

if [ -s "$netr" ]; then
  while IFS="$(printf "\t")" read -r nic rx tx; do
    [ -n "$nic" ] || continue;
    [ -e "/sys/class/net/$nic/device" ] || continue;

    state=$(cat "/sys/class/net/$nic/operstate" 2>/dev/null || echo unknown);
    [ "$state" = "up" ] || continue;

    printf "NETWORK\t%s\t%s\t%s\t%s\n" "$nic" "$state" "$rx" "$tx";
    found_net=1;
  done <"$netr";
fi;

[ "$found_net" -eq 1 ] || printf "NETWORK\t-\t-\t0\t0\n";

if command -v findmnt >/dev/null 2>&1; then
  findmnt -b -rn -o SOURCE,TARGET,FSTYPE,SIZE,AVAIL,USE% 2>/dev/null | awk '"'"'
  BEGIN {
    OFS="\t";
  }
  {
    src=$1;
    mp=$2;
    fstype=$3;
    total=$4;
    avail=$5;
    usep=$6;

    if (src !~ "^/dev/") next;
    if (mp=="" || mp=="-") next;
    if (seen[mp]++) next;

    if (fstype ~ /^(tmpfs|devtmpfs|squashfs|overlay|proc|sysfs|cgroup|cgroup2|devpts|securityfs|pstore|bpf|tracefs|debugfs|mqueue|hugetlbfs|fusectl|configfs|autofs|ramfs|binfmt_misc)$/) next;

    gsub(/%/, "", usep);

    printf "%s\t%s\t%s\t%s\t%s\n", src, mp, total, avail, usep;
  }
  '"'"' >"$diskf";
elif command -v df >/dev/null 2>&1; then
  df -B1 -P 2>/dev/null | awk '"'"'
  BEGIN {
    OFS="\t";
  }
  NR>1 {
    src=$1;
    total=$2;
    avail=$4;
    usep=$5;
    mp=$6;

    if (src !~ "^/dev/") next;
    if (mp=="" || mp=="-") next;
    if (seen[mp]++) next;

    gsub(/%/, "", usep);

    printf "%s\t%s\t%s\t%s\t%s\n", src, mp, total, avail, usep;
  }
  '"'"' >"$diskf";
else
  : >"$diskf";
fi;

if [ -s "$diskf" ]; then
  while IFS="$(printf "\t")" read -r disk mp total avail usep; do
    [ -n "$disk" ] || continue;
    printf "DISK\t%s\t%s\t%s\t%s\t%s\n" "$disk" "$mp" "$total" "$avail" "$usep";
  done <"$diskf";
else
  printf "DISK\t-\t-\t0\t0\t0\n";
fi
'"#;

impl RemoteStatsService {
    pub fn new(config: SshSessionConfig) -> Self {
        Self {
            config,
            multiplex: None,
        }
    }

    pub fn with_multiplex(
        config: SshSessionConfig,
        multiplex: SshMultiplexHandle,
    ) -> anyhow::Result<Self> {
        multiplex.ensure_matches_config(&config)?;
        Ok(Self {
            config,
            multiplex: Some(multiplex),
        })
    }

    pub fn snapshot(&self) -> anyhow::Result<RemoteStats> {
        if !self.config.remote_stats_enabled() {
            anyhow::bail!("remote stats are disabled for this SSH profile");
        }
        let output = self.exec_success(SYSINFO_SCRIPT, "Failed to fetch stats")?;
        Ok(parse_stats_output(&output.stdout))
    }

    fn exec_success(&self, command: &str, context: &str) -> anyhow::Result<RemoteCommandOutput> {
        let output = run_ssh_command(
            self.config.clone(),
            self.multiplex.clone(),
            command.as_bytes().to_vec(),
            STATS_TIMEOUT,
        )?;
        ensure_remote_command_success(output, context)
    }
}

pub fn parse_stats_output(output: &str) -> RemoteStats {
    let mut stats = RemoteStats::default();
    let mut seen_disk_mounts = HashSet::new();

    for line in output.lines() {
        let cols: Vec<&str> = line.split('\t').collect();

        if cols.is_empty() {
            continue;
        }

        match cols[0] {
            "SYSTEM" if cols.len() >= 5 => {
                stats.system = SystemInfo {
                    hostname: cols[1].to_string(),
                    uptime_sec: cols[2].parse().unwrap_or(0),
                    os: cols[3].to_string(),
                    arch: cols[4].to_string(),
                };
            }
            "LOAD" if cols.len() >= 4 => {
                stats.load = LoadInfo {
                    load1: cols[1].parse().unwrap_or(0.0),
                    load5: cols[2].parse().unwrap_or(0.0),
                    load15: cols[3].parse().unwrap_or(0.0),
                };
            }
            "CPU" if cols.len() >= 4 => {
                stats.cpu = CpuInfo {
                    model: cols[1].to_string(),
                    cores: cols[2].parse().unwrap_or(0),
                    usage: cols[3].parse().unwrap_or(0.0),
                    per_core: Vec::new(),
                };
            }
            "CPUCORE" if cols.len() >= 3 => {
                let usage: f64 = cols[2].parse().unwrap_or(0.0);
                stats.cpu.per_core.push(usage);
            }
            "MEMORY" if cols.len() >= 4 => {
                stats.memory = MemoryInfo {
                    used: cols[1].parse().unwrap_or(0),
                    available: cols[2].parse().unwrap_or(0),
                    cached: cols[3].parse().unwrap_or(0),
                };
            }
            "NETWORK" if cols.len() >= 5 => {
                if cols[1] != "-" {
                    stats.networks.push(NetworkInfo {
                        nic: cols[1].to_string(),
                        state: cols[2].to_string(),
                        rx_bytes_per_sec: cols[3].parse().unwrap_or(0.0),
                        tx_bytes_per_sec: cols[4].parse().unwrap_or(0.0),
                    });
                }
            }
            "DISK" if cols.len() >= 6 && cols[1] != "-" => {
                let mount = cols[2].trim();
                if mount.is_empty() || mount == "-" {
                    continue;
                }
                if seen_disk_mounts.insert(mount.to_string()) {
                    stats.disks.push(DiskInfo {
                        device: cols[1].to_string(),
                        mount: mount.to_string(),
                        total: cols[3].parse().unwrap_or(0),
                        available: cols[4].parse().unwrap_or(0),
                        use_percent: cols[5].parse().unwrap_or(0),
                    });
                }
            }
            _ => {}
        }
    }

    stats
}

#[cfg(test)]
mod tests {
    use super::parse_stats_output;

    #[test]
    fn parses_remote_stats_and_deduplicates_mounts() {
        let raw = [
            "SYSTEM\tserver-a\t3600\tUbuntu 24.04\tx86_64",
            "LOAD\t0.10\t0.20\t0.30",
            "CPU\tAMD EPYC\t8\t12.5",
            "CPUCORE\t0\t10.0",
            "CPUCORE\t1\t15.0",
            "MEMORY\t1048576\t2097152\t524288",
            "NETWORK\teth0\tup\t128\t256",
            "NETWORK\t-\t-\t0\t0",
            "DISK\t/dev/sda1\t/\t1000\t500\t50",
            "DISK\t/dev/sdb1\t/data\t2000\t1000\t50",
            "DISK\t/dev/dup\t/data\t999\t999\t1",
            "DISK\t-\t-\t0\t0\t0",
        ]
        .join("\n");

        let stats = parse_stats_output(&raw);

        assert_eq!(stats.system.hostname, "server-a");
        assert_eq!(stats.system.uptime_sec, 3600);
        assert_eq!(stats.load.load5, 0.20);
        assert_eq!(stats.cpu.model, "AMD EPYC");
        assert_eq!(stats.cpu.cores, 8);
        assert_eq!(stats.cpu.usage, 12.5);
        assert_eq!(stats.cpu.per_core, vec![10.0, 15.0]);
        assert_eq!(stats.memory.available, 2097152);
        assert_eq!(stats.networks.len(), 1);
        assert_eq!(stats.networks[0].nic, "eth0");
        assert_eq!(stats.disks.len(), 2);
        assert_eq!(stats.disks[1].mount, "/data");
    }

    #[test]
    fn parser_tolerates_invalid_numbers() {
        let raw = [
            "SYSTEM\thost\tbad\tos\tarch",
            "LOAD\tbad\tbad\tbad",
            "CPU\tmodel\tbad\tbad",
            "CPUCORE\t0\tbad",
            "MEMORY\tbad\tbad\tbad",
            "NETWORK\teth0\tup\tbad\tbad",
            "DISK\t/dev/sda1\t/\tbad\tbad\tbad",
        ]
        .join("\n");

        let stats = parse_stats_output(&raw);

        assert_eq!(stats.system.uptime_sec, 0);
        assert_eq!(stats.load.load1, 0.0);
        assert_eq!(stats.cpu.cores, 0);
        assert_eq!(stats.cpu.per_core, vec![0.0]);
        assert_eq!(stats.memory.used, 0);
        assert_eq!(stats.networks[0].rx_bytes_per_sec, 0.0);
        assert_eq!(stats.disks[0].use_percent, 0);
    }
}
