use std::fs::File;
use std::io::{BufRead, BufReader};

pub trait ToCsv {
  fn to_csv_rows(&self) -> Vec<String>;
}

/// Partitions are loaded from `/proc/partitions`.
#[derive(Clone)]
pub struct Partition {
  pub name: String,
  pub dev: u32,
}

impl Partition {
  // Loads current partitions from /proc/partitions
  // https://github.com/eunomia-bpf/bpf-developer-tutorial/blob/main/src/17-biopattern/trace_helpers.c
  // the file has a structure like this
  //
  // major minor  #blocks  name
  //
  //   259     0  250059096 nvme0n1
  //   259     1     524288 nvme0n1p1
  //   259     2   25165824 nvme0n1p2
  //   259     3  224367616 nvme0n1p3
  //     8     0  976762584 sda
  //     8     1  976760832 sda1
  pub fn load() -> Vec<Self> {
    let f = File::open("/proc/partitions").expect("cannot open /proc/partitions");
    let reader = BufReader::new(f);
    let mut partitions = Vec::new();
    for line in reader.lines().map_while(Result::ok) {
      if line.is_empty() || line.starts_with("major") {
        continue;
      }
      partitions.push(Self::from_str(line.trim()));
    }
    partitions
  }

  /// Creates a Partition from a line in `/proc/partitions`
  pub fn from_str(value: &str) -> Self {
    let parts = value.split_whitespace().collect::<Vec<&str>>();
    assert_eq!(parts.len(), 4);
    let major = parts[0].parse::<u32>().unwrap();
    let minor = parts[1].parse::<u32>().unwrap();
    Self {
      name: parts[3].to_string(),
      // https://man7.org/linux/man-pages/man3/makedev.3.html
      dev: major << 20 | minor,
    }
  }
}

/// Disk statistics collected from the eBPF program.
#[derive(Clone)]
pub struct DiskStats {
  pub name: String,
  pub bytes: u64,
  pub perc_random: u32,
  pub perc_seq: u32,
}

impl ToCsv for DiskStats {
  fn to_csv_rows(&self) -> Vec<String> {
    vec![
      // Here we return three rows for this disk:
      format!("disk_write_seq,%,{},{}", self.perc_seq, self.name),
      format!("disk_write_rand,%,{},{}", self.perc_random, self.name),
      format!("disk_tot_written_bytes,%,{},{}", self.bytes, self.name),
    ]
  }
}

/// An event from the deep trace eBPF program.
#[repr(C)]
#[derive(Default, Debug, Clone)]
pub struct DeepTraceEvent {
  pub ev_type: u32,
  pub timestamp: u64,
}

impl ToCsv for DeepTraceEvent {
  fn to_csv_rows(&self) -> Vec<String> {
    let event_str = match self.ev_type {
      0 => "sys-read",
      1 => "sys-write",
      2 => "mm-page-alloc",
      3 => "mm-page-free",
      4 => "kmalloc",
      5 => "kfree",
      6 => "disk-read",
      7 => "disk-write",
      _ => "unknown",
    };
    vec![format!("{},{}", self.timestamp, event_str)]
  }
}

// SGX statistics combining higher-level metrics with low-level counters.
// Gramine stderr
// # of EENTERs:        139328
// # of EEXITs:         139250
// # of AEXs:           5377
// # of sync signals:   72
// # of async signals:  0
#[derive(Default)]
pub struct SGXStats {
  pub eenter: u64,
  pub eexit: u64,
  pub aexit: u64,
  pub sync_signals: u64,
  pub async_signals: u64,
  pub counters: LowLevelSgxCounters,
}

impl ToCsv for SGXStats {
  fn to_csv_rows(&self) -> Vec<String> {
    let mut rows = Vec::new();
    rows.push(format!("sgx_enter,#,{},", self.eenter));
    rows.push(format!("sgx_eexit,#,{},", self.eexit));
    rows.push(format!("sgx_aexit,#,{},", self.aexit));
    rows.push(format!("sgx_sync_signals,#,{},", self.sync_signals));
    rows.push(format!("sgx_async_signals,#,{},", self.async_signals));
    // Append CSV rows from the low-level counters.
    rows.extend(self.counters.to_csv_rows());
    rows
  }
}

/// A low-level view of SGX counters.
#[repr(C)]
#[derive(Default)]
pub struct LowLevelSgxCounters {
  pub encl_load_page: u64,
  pub encl_wb: u64,
  pub vma_access: u64,
  pub vma_fault: u64,
}

impl ToCsv for LowLevelSgxCounters {
  fn to_csv_rows(&self) -> Vec<String> {
    vec![
      format!("sgx_encl_load_page,#,{},", self.encl_load_page),
      format!("sgx_encl_wb,#,{},", self.encl_wb),
      format!("sgx_vma_access,#,{},", self.vma_access),
      format!("sgx_vma_fault,#,{},", self.vma_fault),
    ]
  }
}

/// A sample of energy consumption.
#[derive(Clone, Debug)]
pub struct EnergySample {
  pub timestamp: u128,
  pub energy_uj: u64,
}

impl ToCsv for EnergySample {
  fn to_csv_rows(&self) -> Vec<String> {
    vec![format!("{},{}", self.timestamp, self.energy_uj)]
  }
}

#[cfg(test)]
mod test {
  use crate::stats::Partition;

  #[test]
  fn test_partition_from_string() {
    let raw = r#" 259        0  250059096 nvme0n1"#;
    let partition = Partition::from_str(raw);

    assert_eq!(partition.name, "nvme0n1");
    assert_eq!(partition.dev, 271581184);
  }
}
