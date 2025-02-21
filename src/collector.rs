use std::{
  collections::{HashMap, HashSet},
  env,
  fmt::Debug,
  fs::{self, create_dir_all},
  mem::MaybeUninit,
  path::{Path, PathBuf},
  process::{Child, Command, Stdio},
  sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
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
use tracing::{error, trace, warn};
use utils::{
  extract_rapl_path, get_map_result, get_sgx_stats, process_disk_stats, process_mem_stats,
  run_command_with_args, save_deep_stats, save_energy_data, save_io_metrics, save_perf_output,
  save_stdout_stderr,
};

use crate::{
  constants::DEFAULT_PERF_EVENTS,
  stats::{DeepTraceEvent, DiskStats, EnergySample, LowLevelSgxCounters, Partition, SGXStats},
  tracer::{
    types::{disk_counter, io_counter},
    TracerSkelBuilder,
  },
};
unsafe impl Plain for io_counter {}
unsafe impl Plain for disk_counter {}

pub struct DefaultCollector {
  sample_size: u32,
  deep_trace: bool,
  perf_events: Vec<String>,
  rapl_paths: Vec<(String, PathBuf)>,
  energy_sample_interval: Duration,
  partitions: Vec<Partition>,
  stop: Arc<AtomicBool>,
}

struct TraceResult {
  mem_counters: Vec<(u32, io_counter)>,
  disk_counters: Vec<(u32, disk_counter)>,
  sgx_counters: Option<LowLevelSgxCounters>,

  deep_stats: Option<Vec<DeepTraceEvent>>,
}

struct Metrics {
  energy_stats: HashMap<String, Vec<EnergySample>>,
  stdout: Vec<u8>,
  stderr: Vec<u8>,
  perf_output: Vec<u8>,
  sys_write_count: u64,
  sys_write_avg: u64,
  sys_read_count: u64,
  sys_read_avg: u64,
  disk_stats: Vec<DiskStats>,
  sgx_stats: Option<SGXStats>,
  deep_stats: Option<Vec<DeepTraceEvent>>,
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
      stop: Arc::new(AtomicBool::new(false)),
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
    threads: usize,
  ) -> Result<(), std::io::Error> {
    let is_sgx = program.as_os_str() == "gramine-sgx";

    // skip sgx to speed development on non sgx machine
    if is_sgx && env::var_os("EB_SKIP_SGX").is_some_and(|v| v == "1") {
      return Ok(());
    }

    let cmd = Command::new(program)
      .args(args)
      .env("OMP_NUM_THREADS", threads.to_string())
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
        if let Some(deep_stats) = metrics.deep_stats {
          save_deep_stats(experiment_directory, deep_stats)?;
        }
      }
      Err(e) => error!("cannot start child process {}", e),
    }
    Ok(())
  }

  #[tracing::instrument(level = "trace", skip(self), err)]
  pub fn attach(
    self: Arc<Self>,
    program: PathBuf,
    args: Vec<String>,
    pre_run: Option<(PathBuf, Vec<String>)>,
    post_run: Option<(PathBuf, Vec<String>)>,
    threads: usize,
    output_directory: &Path,
  ) -> Result<(), Box<dyn std::error::Error>> {
    let me = self.clone();
    for n in 1..me.clone().sample_size + 1 {
      if self.stop.clone().load(Ordering::Relaxed) {
        break;
      }
      let experiment_directory = output_directory.join(PathBuf::from(n.to_string()));
      create_dir_all(&experiment_directory)?;

      let span = tracing::span!(tracing::Level::TRACE, "iteration", iteration = n);
      let _enter = span.enter();

      if let Some((cmd, args)) = &pre_run {
        run_command_with_args(cmd, args)?;
      }

      me.clone().run_experiment(
        &program,
        &args,
        experiment_directory.as_path(),
        false,
        threads,
      )?;

      if let Some((cmd, args)) = &post_run {
        run_command_with_args(cmd, args)?;
      }
    }

    if self.deep_trace && !self.stop.clone().load(Ordering::Relaxed) {
      let span = tracing::span!(tracing::Level::TRACE, "deep_trace");
      let _enter = span.enter();
      trace!("entering deep trace");
      let experiment_directory = output_directory.join(PathBuf::from("deep-trace"));
      create_dir_all(&experiment_directory)?;
      me.clone().run_experiment(
        &program,
        &args,
        experiment_directory.as_path(),
        true,
        threads,
      )?;

      trace!("deep trace finished");
    }
    Ok(())
  }

  #[tracing::instrument(level = "trace", skip(self, child))]
  fn collect_metrics(self: Arc<Self>, child: Child, is_sgx: bool, deep_trace: bool) -> Metrics {
    let pid = child.id();
    let stop = Arc::new(AtomicBool::new(false));

    let perf_handle = {
      let me = self.clone();
      thread::spawn(move || me.run_perf(pid))
    };

    let energy_handle = {
      let me = self.clone();
      let stop = stop.clone();
      thread::spawn(move || me.monitor_energy_consumption(&stop))
    };

    let tracing_handle = {
      let me = self.clone();
      let stop = stop.clone();
      thread::spawn(move || me.trace_program(pid, &stop, is_sgx, deep_trace))
    };

    let wait_child_handle = {
      let me = self.clone();
      let stop = stop.clone();
      thread::spawn(move || me.wait_for_child(child, &stop))
    };

    let (stdout, stderr) = wait_child_handle.join().unwrap();
    trace!("target process joined!");

    let trace_result = tracing_handle.join().unwrap();
    trace!("trace thread joined!");

    let energy_stats = energy_handle.join().unwrap();
    trace!("energy thread joined");

    let perf_output = perf_handle.join().unwrap();
    trace!("perf thread joined");

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
      deep_stats: trace_result.deep_stats,
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

  fn monitor_energy_consumption(&self, stop: &AtomicBool) -> HashMap<String, Vec<EnergySample>> {
    let mut measures: HashMap<String, Vec<EnergySample>> = HashMap::new();
    while !stop.load(Ordering::Relaxed) {
      let timestamp = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
      for (name, rapl_path) in &self.rapl_paths {
        if let Ok(energy_str) = fs::read_to_string(rapl_path) {
          // Parse the energy value as a number (assumes the file contains a numeric value)
          if let Ok(energy_uj) = energy_str.trim().parse::<u64>() {
            measures
              .entry(name.to_owned())
              .or_default()
              .push(EnergySample {
                timestamp,
                energy_uj,
              });
          }
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
    stop: &AtomicBool,
    is_sgx: bool,
    deep_trace: bool,
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
      let result = Arc::new(Mutex::new(Vec::new()));
      ring_buffer
        .add(&prog.maps.events, {
          let result = result.clone();
          move |c| -> i32 {
            let deep_trace_event =
              unsafe { std::ptr::read_unaligned(c.as_ptr() as *const DeepTraceEvent) };
            result.lock().unwrap().push(deep_trace_event);
            0
          }
        })
        .unwrap();
      Some((ring_buffer.build().unwrap(), result))
    } else {
      None
    };

    // wait for target program to end
    while !stop.load(Ordering::Relaxed) {
      if let Some((ref mut rb, _)) = maybe_ring_buffer {
        rb.poll(Duration::from_millis(250))
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

    // need to copy because there are problems when extracting from Arc<Mutex<T>>
    let deep_stats = match maybe_ring_buffer {
      Some((_, stats)) => {
        let result = stats.lock().unwrap().clone();
        Some(result)
      }
      None => None,
    };

    TraceResult {
      disk_counters,
      sgx_counters,
      mem_counters,
      deep_stats,
    }
  }

  fn wait_for_child(self: Arc<Self>, child: Child, finished: &AtomicBool) -> (Vec<u8>, Vec<u8>) {
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let child = Mutex::new(child);

    let stop = self.stop.clone();
    while !stop.load(Ordering::Relaxed) {
      if let Ok(Some(status)) = child.lock().unwrap().try_wait() {
        if !status.success() {
          warn!(
            "child process exited with non-zero code {}",
            status
              .code()
              .map_or("unknown".to_string(), |c| c.to_string())
          );
        }
        break;
      }

      thread::sleep(Duration::from_secs(1));
    }
    let mut child = child.lock().unwrap();
    if let Some(mut stdout_pipe) = child.stdout.take() {
      let _ = std::io::copy(&mut stdout_pipe, &mut stdout);
    }
    if let Some(mut stderr_pipe) = child.stderr.take() {
      let _ = std::io::copy(&mut stderr_pipe, &mut stderr);
    }
    if let Err(e) = child.kill() {
      error!("cannot kill child process with pid {}: {}", child.id(), e);
    }

    finished.store(true, Ordering::Relaxed);
    (stdout, stderr)
  }

  pub fn stop(self: Arc<Self>) {
    self.clone().stop.store(true, Ordering::Relaxed);
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
  use tracing::{trace, warn};

  use crate::{
    collector::{DiskStats, Partition, SGXStats},
    constants::{ENERGY_CSV_HEADER, IO_CSV_HEADER, TRACE_CSV_HEADER},
    stats::{EnergySample, ToCsv},
    tracer::types::{disk_counter, io_counter},
  };

  use super::{DeepTraceEvent, LowLevelSgxCounters};

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
    energy_stats: HashMap<String, Vec<EnergySample>>,
  ) -> Result<(), std::io::Error> {
    for (filename, samples) in energy_stats {
      let mut file = File::create(experiment_directory.join(format!("{}.csv", filename)))?;
      writeln!(file, "{}", ENERGY_CSV_HEADER)?;
      let csv_lines: Vec<String> = samples
        .iter()
        .flat_map(|sample| sample.to_csv_rows())
        .collect();
      file.write_all(csv_lines.join("\n").as_bytes())?;
    }
    Ok(())
  }

  pub fn save_io_metrics(
    experiment_directory: &Path,
    metrics: &super::Metrics,
  ) -> Result<(), std::io::Error> {
    let io_path = experiment_directory.join("io.csv");
    let mut file = File::create(&io_path)?;
    writeln!(file, "{}", IO_CSV_HEADER)?;
    if let Some(sgx) = &metrics.sgx_stats {
      // Use the to_csv_rows method for the low-level SGX counters.
      for row in sgx.counters.to_csv_rows() {
        writeln!(file, "{}", row)?;
      }
    }
    writeln!(file, "sys_read,#,{},", metrics.sys_read_count)?;
    writeln!(file, "sys_read,ns,{},", metrics.sys_read_avg)?;
    writeln!(file, "sys_write,#,{},", metrics.sys_write_count)?;
    writeln!(file, "sys_write,ns,{},", metrics.sys_write_avg)?;

    // Now use the DiskStats to_csv_rows method.
    for stats in &metrics.disk_stats {
      for row in stats.to_csv_rows() {
        writeln!(file, "{}", row)?;
      }
    }
    Ok(())
  }

  pub fn save_deep_stats(
    experiment_directory: &Path,
    stats: Vec<DeepTraceEvent>,
  ) -> Result<(), std::io::Error> {
    let trace_path = experiment_directory.join("trace.csv");
    let mut file = File::create(&trace_path)?;
    writeln!(file, "{}", TRACE_CSV_HEADER)?;
    // Use the new `to_csv_rows` method from our types.
    let csv_rows: Vec<String> = stats.iter().flat_map(|e| e.to_csv_rows()).collect();

    write!(file, "{}", csv_rows.join("\n"))?;
    Ok(())
  }

  pub fn run_command_with_args(cmd: &PathBuf, args: &[String]) -> Result<(), std::io::Error> {
    let output = Command::new(cmd)
      .args(args)
      .stdout(Stdio::piped())
      .stderr(Stdio::piped())
      .output()?;

    if !output.status.success() {
      let code = output
        .status
        .code()
        .map_or(String::from("unknown"), |c| c.to_string());
      warn!(
        "command {:?} exited with status {}: {} {}",
        cmd.to_string_lossy(),
        code,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
      );
    } else {
      trace!("command {:?} terminated with exit code 0", cmd)
    }
    Ok(())
  }
}

#[cfg(test)]
mod test {
  use std::{path::PathBuf, sync::Arc, time::Duration};

  use tempfile::TempDir;

  use super::DefaultCollector;

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
        1,
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
}
