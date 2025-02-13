#![allow(dead_code)]
use std::{
  collections::{HashMap, HashSet},
  env,
  fmt::Debug,
  fs::{self, create_dir_all, File},
  io::{BufRead, BufReader},
  mem::MaybeUninit,
  path::{Path, PathBuf},
  process::{Child, Command, Stdio},
  sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
  },
  thread,
  time::{Duration, SystemTime},
};

use duration_str::HumanFormat;
use libbpf_rs::{
  skel::{OpenSkel, Skel, SkelBuilder},
  MapCore, MapFlags, RingBufferBuilder,
};
use plain::Plain;
use tracing::{debug, error, trace, warn};
use utils::{
  extract_rapl_path, get_map_result, get_sgx_stats, process_disk_stats, process_mem_stats,
  run_command_with_args, save_energy_data, save_io_metrics, save_perf_output, save_stdout_stderr,
};

use crate::{
  constants::DEFAULT_PERF_EVENTS,
  tracer::{
    types::{disk_counter, io_counter},
    TracerSkelBuilder,
  },
};
unsafe impl Plain for io_counter {}
unsafe impl Plain for disk_counter {}

#[cfg(target_os = "linux")]
#[derive(Clone)]
struct Partition {
  name: String,
  dev: u32,
}

#[cfg(target_os = "linux")]
pub struct DefaultCollector {
  sample_size: u32,
  deep_trace: bool,
  perf_events: Vec<String>,
  rapl_paths: Vec<(String, PathBuf)>,
  energy_sample_interval: Duration,
  partitions: Vec<Partition>,
}

struct DiskStats {
  name: String,
  bytes: u64,
  perc_random: u32,
  perc_seq: u32,
}

struct TraceResult {
  mem_counters: Vec<(u32, io_counter)>,
  disk_counters: Vec<(u32, disk_counter)>,
  sgx_counters: Option<LowLevelSgxCounters>,

  deep_stats: Option<Vec<(String, u64)>>,
}

// # of EENTERs:        139328
// # of EEXITs:         139250
// # of AEXs:           5377
// # of sync signals:   72
// # of async signals:  0
#[derive(Default)]
struct SGXStats {
  eenter: u64,
  eexit: u64,
  aexit: u64,
  sync_signals: u64,
  async_signals: u64,
  counters: LowLevelSgxCounters,
}

#[repr(C)]
#[derive(Default)]
struct LowLevelSgxCounters {
  encl_load_page: u64,
  encl_wb: u64,
  vma_access: u64,
  vma_fault: u64,
}

struct DeepStats {
  cpu_events: Vec<(u64, u64)>,
}

struct Metrics {
  energy_stats: HashMap<String, Vec<String>>,
  stdout: Vec<u8>,
  stderr: Vec<u8>,
  perf_output: Vec<u8>,
  sys_write_count: u64,
  sys_write_avg: u64,
  sys_read_count: u64,
  sys_read_avg: u64,
  disk_stats: Vec<DiskStats>,
  sgx_stats: Option<SGXStats>,
  deep_stats: Option<DeepStats>,
}

impl DefaultCollector {
  pub fn new(
    sample_size: u32,
    deep_trace: bool,
    energy_sample_interval: Duration,
    extra_perf_events: Option<Vec<String>>,
  ) -> Self {
    Self {
      sample_size,
      partitions: Partition::load(),
      deep_trace,
      energy_sample_interval,
      perf_events: {
        let mut perf_events: HashSet<String> =
          HashSet::from_iter(DEFAULT_PERF_EVENTS.iter().map(|v| v.to_string()));
        for extra_perf_event in extra_perf_events.unwrap_or_default() {
          perf_events.insert(extra_perf_event);
        }
        Vec::from_iter(perf_events.iter().map(String::from))
      },
      // discovery rapl paths: https://www.kernel.org/doc/html/next/power/powercap/powercap.html
      rapl_paths: {
        let base_path = Path::new("/sys/devices/virtual/powercap/intel-rapl");
        let mut rapl_paths = Vec::new();

        if base_path.is_dir() {
          for entry in base_path.read_dir().unwrap().flatten() {
            if let Some(s) = extract_rapl_path(&entry) {
              let domain_name = s.0.clone();
              rapl_paths.push(s);
              for subentry in entry.path().read_dir().unwrap().flatten() {
                if let Some(r) = extract_rapl_path(&subentry) {
                  let name = format!("{}-{}", domain_name, r.0);
                  rapl_paths.push((name, r.1));
                }
              }
            }
          }
        } else {
          warn!("system does not support RAPL interface; skipping");
        }
        rapl_paths
      },
    }
  }

  #[tracing::instrument(level = "trace", skip(self), err)]
  fn run_experiment(
    self: Arc<Self>,
    program: &PathBuf,
    args: &[String],
    experiment_directory: &Path,
    deep_trace: bool,
  ) -> Result<(), Box<dyn std::error::Error>> {
    let is_sgx = program.as_os_str() == "gramine-sgx";

    // skip sgx to speed development on non sgx machine
    if is_sgx && env::var_os("EB_SKIP_SGX").is_some_and(|v| v == "1") {
      debug!("EB_SKIP_SGX is set; skipping SGX execution");
      return Ok(());
    }

    let cmd = Command::new(program)
      .args(args)
      .stdout(Stdio::piped())
      .stderr(Stdio::piped())
      .spawn();

    match cmd {
      Ok(child) => {
        let metrics = self.collect_metrics(child, is_sgx, deep_trace);

        save_perf_output(experiment_directory, &metrics.perf_output)?;
        save_stdout_stderr(experiment_directory, &metrics.stdout, &metrics.stderr)?;
        save_energy_data(experiment_directory, metrics.energy_stats.clone())?;
        save_io_metrics(experiment_directory, &metrics)?;
      }
      Err(e) => error!("cannot start child process {}", e),
    }
    Ok(())
  }

  #[tracing::instrument(level = "debug", skip(self), err)]
  pub fn attach(
    self: Arc<Self>,
    program: PathBuf,
    args: Vec<String>,
    pre_run: Option<(PathBuf, Vec<String>)>,
    post_run: Option<(PathBuf, Vec<String>)>,
    output_directory: &Path,
  ) -> Result<(), Box<dyn std::error::Error>> {
    let me = self.clone();
    for n in 1..me.clone().sample_size + 1 {
      let experiment_directory = output_directory.join(PathBuf::from(n.to_string()));
      create_dir_all(&experiment_directory)?;

      if let Some((cmd, args)) = &pre_run {
        run_command_with_args(cmd, args)?;
      }

      me.clone()
        .run_experiment(&program, &args, experiment_directory.as_path(), false)?;

      if let Some((cmd, args)) = &post_run {
        run_command_with_args(cmd, args)?;
      }
    }

    if self.deep_trace {
      trace!("entering deep trace; this may take some time...");
      let experiment_directory = output_directory.join(PathBuf::from("deep-trace"));
      create_dir_all(&experiment_directory)?;
      me.clone()
        .run_experiment(&program, &args, experiment_directory.as_path(), true)?;

      trace!("deep trace finished");
    }
    Ok(())
  }

  #[tracing::instrument(level = "trace", skip(self, child))]
  fn collect_metrics(self: Arc<Self>, child: Child, is_sgx: bool, deep_trace: bool) -> Metrics {
    let stop = Arc::new(AtomicBool::new(false));
    let pid = child.id();

    let perf_handle = {
      let me = self.clone();
      thread::spawn(move || me.run_perf(pid))
    };

    let energy_handle = {
      let me = self.clone();
      let energy_stop = stop.clone();
      thread::spawn(move || me.monitor_energy_consumption(&energy_stop))
    };

    let tracing_handle = {
      let me = self.clone();
      let tracing_stop = stop.clone();
      thread::spawn(move || me.trace_program(pid, is_sgx, deep_trace, tracing_stop))
    };

    let wait_child_handle = {
      let me = self.clone();
      let stop = stop.clone();
      thread::spawn(move || me.wait_for_child(child, stop))
    };

    let trace_result = tracing_handle.join().unwrap();

    let (stdout, stderr) = wait_child_handle.join().unwrap();
    let energy_stats = energy_handle.join().unwrap();
    let perf_output = perf_handle.join().unwrap();

    let disk_stats = process_disk_stats(&self.partitions, trace_result.disk_counters);
    let (sys_write_count, sys_write_avg, sys_read_count, sys_read_avg) =
      process_mem_stats(trace_result.mem_counters);

    let sgx_stats = trace_result
      .sgx_counters
      .map(|sgx_counters| get_sgx_stats(&stderr, sgx_counters));

    Metrics {
      stdout,
      stderr,
      perf_output,
      energy_stats,
      disk_stats,
      sgx_stats,
      sys_read_avg,
      sys_write_avg,
      sys_read_count,
      sys_write_count,
      deep_stats: None,
    }
  }

  fn run_perf(&self, pid: u32) -> Vec<u8> {
    let mut perf_output = Vec::new();
    let mut perf_cmd = Command::new("perf");
    perf_cmd
      .arg("stat")
      .arg("--field-separator")
      .arg(",")
      .arg("--event")
      .arg(self.perf_events.join(","))
      .arg("--pid")
      .arg(pid.to_string())
      .stdout(Stdio::piped())
      .stderr(Stdio::piped());

    match perf_cmd.output() {
      Ok(output) => {
        if !output.status.success() {
          error!(
            "perf process exited with non-zero code {}: {} {}",
            output
              .status
              .code()
              .map_or("unknown".to_string(), |c| c.to_string()),
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
          );
        }
        perf_output = output.stderr;
      }
      Err(e) => error!("perf process error {e}"),
    };

    perf_output
  }

  fn monitor_energy_consumption(&self, stop: &AtomicBool) -> HashMap<String, Vec<String>> {
    let mut measures: HashMap<String, Vec<String>> = HashMap::new();
    while !stop.load(Ordering::Relaxed) {
      let timestamp = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
      for (name, rapl_path) in &self.rapl_paths {
        if let Ok(energy_uj) = fs::read_to_string(rapl_path) {
          measures.entry(name.to_owned()).or_default().push(format!(
            "{},{}",
            timestamp,
            energy_uj.trim()
          ));
        }
      }
      thread::sleep(self.energy_sample_interval);
    }
    measures
  }

  #[allow(clippy::type_complexity)]
  fn trace_program(
    &self,
    pid: u32,
    is_sgx: bool,
    deep_trace: bool,
    tracing_stop: Arc<AtomicBool>,
  ) -> TraceResult {
    let skel_builder = TracerSkelBuilder::default();
    let mut open_object = MaybeUninit::uninit();
    let open_skel = skel_builder
      .open(&mut open_object)
      .expect("cannot open ebpf program");
    open_skel.maps.rodata_data.targ_pid = pid as i32;
    open_skel.maps.rodata_data.deep_trace = deep_trace;
    trace!(
      "attaching ebpf program on target process with pid {}",
      pid as i32
    );
    let mut prog = open_skel.load().expect("cannot load ebpf program");
    prog.attach().expect("cannot attach program");

    let mut maybe_ring_buffer = if deep_trace {
      let mut ring_buffer = RingBufferBuilder::new();
      ring_buffer
        .add(&prog.maps.events, move |_c| -> i32 { 0 })
        .unwrap();
      println!("built ring buffer");
      Some(ring_buffer.build().unwrap())
    } else {
      None
    };

    // wait for target program to end
    while !tracing_stop.load(Ordering::Relaxed) {
      if let Some(ref mut rb) = maybe_ring_buffer {
        println!("polling");
        rb.poll(Duration::from_millis(500))
          .expect("cannot poll from ring buffer");
      } else {
        thread::sleep(Duration::from_secs(1));
      }
    }

    let mem_counters = get_map_result::<u32, io_counter>(
      &prog.maps.agg_map,
      Some(&|key, value| {
        trace!(
          "got {} {} operations; average duration {}ns",
          value.count,
          if *key == 0 { "write" } else { "read" },
          value.total_duration.checked_div(value.count).unwrap_or(0)
        );
      }),
    );
    let disk_counters = get_map_result::<u32, disk_counter>(
      &prog.maps.counters,
      Some(&|key, value| {
        let total = value.sequential + value.random;

        let partition_name = &self
          .partitions
          .iter()
          .find(|p| p.dev == *key)
          .map_or("unknown", |p| &p.name);

        trace!(
          "dev={} random={}% seq={}% total={} bytes={}",
          partition_name,
          (value.random * 100).checked_div(total).unwrap_or(0),
          (value.sequential * 100).checked_div(total).unwrap_or(0),
          total,
          value.bytes / 1024
        );
      }),
    );

    let key_bytes = 0_i32.to_ne_bytes();

    let sgx_counters = if is_sgx {
      prog
        .maps
        .sgx_stats
        .lookup(&key_bytes, MapFlags::ANY)
        .ok()
        .flatten()
        .map(|val_bytes| {
          // Safety: The size of LowLevelSGX is known; ensure that val_bytes has at least that many bytes.
          unsafe { std::ptr::read_unaligned(val_bytes.as_ptr() as *const LowLevelSgxCounters) }
        })
        .or(Some(LowLevelSgxCounters::default()))
    } else {
      None
    };
    TraceResult {
      disk_counters,
      sgx_counters,
      mem_counters,
      deep_stats: None,
    }
  }

  fn wait_for_child(&self, child: Child, stop: Arc<AtomicBool>) -> (Vec<u8>, Vec<u8>) {
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    match child.wait_with_output() {
      Ok(output) => {
        if !output.status.success() {
          warn!(
            "child process exited with non-zero code {}",
            output
              .status
              .code()
              .map_or("unknown".to_string(), |c| c.to_string())
          );
        }
        stderr = output.stderr;
        stdout = output.stdout;
      }
      Err(e) => error!("target program exited with error {e}"),
    }
    stop.store(true, std::sync::atomic::Ordering::Relaxed);
    (stdout, stderr)
  }
}

impl Debug for DefaultCollector {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    write!(
            f,
            "Default linux debug:\n perf_events={}\n rapl_paths={}\n sample_size={}\n energy_sample_interval={}",
            self.perf_events.join(","),
            self.rapl_paths
                .iter()
                .map(|(_, p)| p.to_str().unwrap().to_string())
                .collect::<Vec<String>>()
                .join(","),
            self.sample_size,
            self.energy_sample_interval.human_format()
        )
  }
}

impl From<&str> for Partition {
  fn from(value: &str) -> Self {
    let parts = value.split_whitespace().collect::<Vec<&str>>();
    assert_eq!(parts.len(), 4);

    let major = parts[0].parse::<u32>().unwrap();
    let minor = parts[1].parse::<u32>().unwrap();

    Partition {
      name: parts[3].parse().unwrap(),
      // https://man7.org/linux/man-pages/man3/makedev.3.html
      dev: major << 20 | minor,
    }
  }
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
    #[allow(clippy::manual_flatten)]
    for line in reader.lines() {
      if let Ok(line) = line {
        // skip first 2 lines
        if line.is_empty() || line.starts_with("major") {
          continue;
        }
        partitions.push(Partition::from(line.trim()));
      }
    }
    partitions
  }
}

mod utils {
  use std::{
    collections::HashMap,
    fs::{self, DirEntry, File},
    io::{BufRead, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
  };

  use libbpf_rs::{Map, MapCore, MapFlags};
  use plain::Plain;
  use tracing::warn;

  use crate::{
    collector::{DiskStats, Partition, SGXStats},
    constants::{ENERGY_CSV_HEADER, IO_CSV_HEADER},
    tracer::types::{disk_counter, io_counter},
  };

  use super::{LowLevelSgxCounters, Metrics};

  pub fn get_sgx_stats(stderr: &[u8], sgx_counters: LowLevelSgxCounters) -> SGXStats {
    let mut sgx_stats = extract_sgx_counters_from_stderr(stderr);

    sgx_stats.counters = sgx_counters;

    sgx_stats
  }

  fn extract_sgx_counters_from_stderr(stderr: &[u8]) -> SGXStats {
    let mut counters = SGXStats::default();
    for line in stderr.lines().map_while(Result::ok) {
      if line.trim().starts_with("#") {
        let parts = line.as_str().split_whitespace().collect::<Vec<&str>>();
        match parts[2] {
          "EENTERs:" => counters.eenter = parts[3].parse().unwrap(),
          "EEXITs:" => counters.eexit = parts[3].parse().unwrap(),
          "AEXs" => counters.aexit = parts[3].parse().unwrap(),
          "sync" => counters.sync_signals = parts[4].parse().unwrap(),
          "async" => counters.async_signals = parts[4].parse().unwrap(),
          _ => {}
        }
      }
    }
    counters
  }

  pub fn process_disk_stats(
    partitions: &[Partition],
    disk_stats: Vec<(u32, disk_counter)>,
  ) -> Vec<DiskStats> {
    disk_stats
      .into_iter()
      .map(|(devid, stats)| {
        let total = stats.random + stats.sequential;
        let name = partitions
          .iter()
          .find(|p| p.dev == devid)
          .map_or("unknown device".to_string(), |p| p.name.clone());
        DiskStats {
          name,
          bytes: stats.bytes,
          perc_random: (stats.random * 100).checked_div(total).unwrap_or(0),
          perc_seq: (stats.sequential * 100).checked_div(total).unwrap_or(0),
        }
      })
      .collect::<Vec<DiskStats>>()
  }

  pub fn process_mem_stats(mem_stats: Vec<(u32, io_counter)>) -> (u64, u64, u64, u64) {
    let (mut sys_write_count, mut sys_write_avg) = (0, 0);
    let (mut sys_read_count, mut sys_read_avg) = (0, 0);
    for (op, stat) in mem_stats {
      match op {
        0 => {
          sys_write_count = stat.count;
          sys_write_avg = stat.total_duration.checked_div(stat.count).unwrap_or(0);
        }
        1 => {
          sys_read_count = stat.count;
          sys_read_avg = stat.total_duration.checked_div(stat.count).unwrap_or(0);
        }
        _ => panic!("unknown system call type expected 0 for READ and 1 for WRITE"),
      }
    }
    (sys_write_count, sys_write_avg, sys_read_count, sys_read_avg)
  }

  pub fn extract_rapl_path(entry: &DirEntry) -> Option<(String, PathBuf)> {
    if entry
      .file_name()
      .to_string_lossy()
      .starts_with("intel-rapl:")
      && entry.path().is_dir()
    {
      let component = fs::read_to_string(entry.path().join("name"))
        .unwrap()
        .trim()
        .to_owned();
      let energy_uj_path = entry.path().join("energy_uj");
      Some((component, energy_uj_path))
    } else {
      None
    }
  }

  #[allow(clippy::type_complexity)]
  pub fn get_map_result<K, T>(map: &Map, cb: Option<&dyn Fn(&K, &T)>) -> Vec<(K, T)>
  where
    K: Plain + Clone,
    T: Plain + Default,
  {
    let mut result = Vec::new();
    for key in map.keys() {
      let value = map
        .lookup(&key, MapFlags::ANY)
        .expect("cannot read from aggregated map");

      if let Some(bytes) = value {
        let mut value = T::default();
        let key = K::from_bytes(&key).expect("cannot convert map key");
        plain::copy_from_bytes(&mut value, &bytes).expect("cannot get key");

        if let Some(cb) = cb {
          cb(key, &value);
        }
        result.push((key.clone(), value));
      }
    }
    result
  }

  pub fn save_perf_output(
    experiment_directory: &Path,
    perf_output: &[u8],
  ) -> Result<(), std::io::Error> {
    std::fs::write(experiment_directory.join("perf.csv"), perf_output)
  }

  pub fn save_stdout_stderr(
    experiment_directory: &Path,
    stdout: &[u8],
    stderr: &[u8],
  ) -> Result<(), std::io::Error> {
    std::fs::write(experiment_directory.join("stdout"), stdout)?;
    std::fs::write(experiment_directory.join("stderr"), stderr)
  }

  pub fn save_energy_data(
    experiment_directory: &Path,
    energy_stats: HashMap<String, Vec<String>>,
  ) -> Result<(), std::io::Error> {
    for (filename, data) in energy_stats {
      let mut file = File::create(experiment_directory.join(format!("{}.csv", filename)))?;
      writeln!(file, "{}", ENERGY_CSV_HEADER)?;
      file.write_all(data.join("\n").as_bytes())?;
    }
    Ok(())
  }

  pub fn save_io_metrics(
    experiment_directory: &Path,
    metrics: &Metrics,
  ) -> Result<(), std::io::Error> {
    let mut file = File::create(experiment_directory.join("io.csv"))?;
    writeln!(file, "{}", IO_CSV_HEADER)?;
    if let Some(sgx) = &metrics.sgx_stats {
      writeln!(file, "sgx_enter,#,{},,", sgx.eenter)?;
      writeln!(file, "sgx_eexit,#,{},,", sgx.eexit)?;
      writeln!(file, "sgx_aexit,#,{},,", sgx.aexit)?;
      writeln!(file, "sgx_async_signals,#,{},,", sgx.async_signals)?;
      writeln!(file, "sgx_sync_signals,#,{},,", sgx.sync_signals)?;
      writeln!(
        file,
        "sgx_encl_load_page,#,{},,",
        sgx.counters.encl_load_page
      )?;
      writeln!(file, "sgx_encl_wb,#,{},,", sgx.counters.encl_wb)?;
      writeln!(file, "sgx_vma_access,#,{},,", sgx.counters.vma_access)?;
      writeln!(file, "sgx_vma_fault,#,{},,", sgx.counters.vma_fault)?;
    }
    writeln!(file, "sys_read,#,{},,", metrics.sys_read_count)?;
    writeln!(file, "sys_read,ns,{},,", metrics.sys_read_avg)?;
    writeln!(file, "sys_write,#,{},,", metrics.sys_write_count)?;
    writeln!(file, "sys_write,ns,{},,", metrics.sys_write_avg)?;

    for stats in &metrics.disk_stats {
      writeln!(file, "disk_write_seq,%,{},{}", stats.perc_seq, stats.name)?;
      writeln!(
        file,
        "disk_write_rand,%,{},{}",
        stats.perc_random, stats.name
      )?;
      writeln!(
        file,
        "disk_tot_written_bytes,%,{},{}",
        stats.bytes, stats.name
      )?;
    }
    Ok(())
  }

  pub fn run_command_with_args(
    cmd: &PathBuf,
    args: &[String],
  ) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(code) = Command::new(cmd)
      .args(args)
      .stdout(Stdio::null())
      .stderr(Stdio::null())
      .status()?
      .code()
    {
      if code != 0 {
        warn!(
          "command {:?} exited with status {}",
          cmd.to_string_lossy(),
          code
        );
      }
    }
    Ok(())
  }
}

#[cfg(test)]
mod test {
  use std::{path::PathBuf, sync::Arc, time::Duration};

  use tempfile::TempDir;

  use super::{DefaultCollector, Partition};

  #[test]
  fn test_collector() {
    let output_directory = TempDir::new().unwrap();
    let sample_size = 1;
    let collector = DefaultCollector::new(sample_size, false, Duration::from_micros(500), None);
    let collector = Arc::new(collector);
    collector
      .clone()
      .attach(
        PathBuf::from("/bin/sleep"),
        vec!["1".to_string()],
        None,
        None,
        output_directory.path(),
      )
      .unwrap();

    for i in 1..sample_size + 1 {
      let iter_directory = output_directory.path().join(i.to_string());
      assert!(iter_directory.join("perf.csv").is_file());
      assert!(iter_directory.join("io.csv").is_file());
      assert!(iter_directory.join("stdout").is_file());
      assert!(iter_directory.join("stderr").is_file());
      for (name, _) in &collector.rapl_paths {
        assert!(iter_directory.join(format!("{}.csv", name)).is_file())
      }
    }
  }

  #[test]
  fn test_partition_from_string() {
    let raw = r#" 259        0  250059096 nvme0n1"#;
    let partition = Partition::from(raw);

    assert_eq!(partition.name, "nvme0n1");
    assert_eq!(partition.dev, 271581184);
  }
}
