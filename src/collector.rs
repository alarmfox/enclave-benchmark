use std::{
    collections::{HashMap, HashSet},
    fmt::Debug,
    fs::{self, create_dir_all, DirEntry, File},
    io::{BufRead, BufReader, Write},
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
    Map, MapCore, MapFlags,
};
use plain::{Error, Plain};
use tracer::{
    types::{disk_counter, io_counter},
    TracerSkelBuilder,
};
use tracing::{error, trace, warn};

use crate::constants::{DEFAULT_PERF_EVENTS, ENERGY_CSV_HEADER, IO_CSV_HEADER};

mod tracer {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/src/bpf/tracer.skel.rs"
    ));
}

unsafe impl Plain for tracer::types::io_counter {}
unsafe impl Plain for tracer::types::disk_counter {}

#[cfg(target_os = "linux")]
#[derive(Clone)]
struct Partition {
    name: String,
    dev: u32,
}

#[cfg(target_os = "linux")]
pub struct DefaultCollector {
    sample_size: u32,
    perf_events: Vec<String>,
    rapl_paths: Vec<(String, PathBuf)>,
    energy_sample_interval: Duration,
    partitions: Vec<Partition>,
}

struct DiskStats {
    name: String,
    bytes: u64,
    perc_random: i32,
    perc_seq: i32,
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
}

struct Metrics {
    energy_stats: HashMap<String, Vec<String>>,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
    sys_write_count: u64,
    sys_write_avg: u64,
    sys_read_count: u64,
    sys_read_avg: u64,
    disk_stats: Vec<DiskStats>,
    sgx_stats: Option<SGXStats>,
}

impl DefaultCollector {
    pub fn new(
        sample_size: u32,
        energy_sample_interval: Duration,
        extra_perf_events: Option<Vec<String>>,
    ) -> Self {
        Self {
            sample_size,
            partitions: Partition::load(),
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
                        match extract_rapl_path(&entry) {
                            //found a path like /sys/devices/virtual/powercap/intel-rapl/intel-rapl:<num>/
                            Some(s) => {
                                let domain_name = s.0.clone();
                                rapl_paths.push(s);
                                for subentry in entry.path().read_dir().unwrap().flatten() {
                                    // /sys/devices/virtual/powercap/intel-rapl/intel-rapl:<num>/intel-rapl:<num>
                                    if let Some(r) = extract_rapl_path(&subentry) {
                                        let name = format!("{}-{}", domain_name, r.0);
                                        rapl_paths.push((name, r.1));
                                    };
                                }
                            }
                            None => continue,
                        };
                    }
                } else {
                    warn!("system does not support RAPL interface; skipping");
                }
                rapl_paths
            },
        }
    }

    #[tracing::instrument(level = "trace", skip(self))]
    fn run_experiment(
        self: Arc<Self>,
        program: &PathBuf,
        args: &[String],
        experiment_directory: &Path,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let is_sgx = program.as_os_str() == "gramine-sgx";

        let cmd = Command::new(program)
            .args(args)
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn();

        match cmd {
            Ok(child) => {
                Command::new("perf")
                    .arg("stat")
                    .arg("--field-separator")
                    .arg(",")
                    .arg("--event")
                    .arg(self.perf_events.join(","))
                    .arg("--output")
                    .arg(experiment_directory.join("perf.csv"))
                    .arg("--pid")
                    .arg(child.id().to_string())
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .spawn()?;

                let metrics = self.collect_metrics(child, is_sgx);

                // write metrics to files
                // save stdout stderr
                std::fs::write(experiment_directory.join("stderr"), metrics.stderr).unwrap();
                std::fs::write(experiment_directory.join("stdout"), metrics.stdout).unwrap();

                // save energy_data
                for (filename, data) in metrics.energy_stats {
                    let mut file =
                        File::create(experiment_directory.join(format!("{}.csv", filename)))
                            .unwrap();
                    writeln!(file, "{}", ENERGY_CSV_HEADER).unwrap();
                    file.write_all(data.join("\n").as_bytes()).unwrap();
                }

                // write i/o metrics
                let mut file = File::create(experiment_directory.join("io.csv")).unwrap();
                writeln!(file, "{}", IO_CSV_HEADER).unwrap();
                if let Some(sgx) = metrics.sgx_stats {
                    writeln!(file, "sgx-enter,#,{},", sgx.eenter).unwrap();
                    writeln!(file, "sgx-eexit,#,{},", sgx.eexit).unwrap();
                    writeln!(file, "sgx-aexit,#,{},", sgx.aexit).unwrap();
                    writeln!(
                        file,
                        "sgx-async-signals,#,{},", sgx.async_signals
                    )
                    .unwrap();
                    writeln!(
                        file,
                        "sgx-sync-signals,#,{},", sgx.sync_signals
                    )
                    .unwrap();
                }
                writeln!(
                    file,
                    "sys_read,#,{},", metrics.sys_read_count
                )
                .unwrap();
                writeln!(
                    file,
                    "sys_read,ns,{},", metrics.sys_read_avg
                )
                .unwrap();
                writeln!(
                    file,
                    "sys_write,#,{},", metrics.sys_write_count
                )
                .unwrap();
                writeln!(
                    file,
                    "sys_write,ns,{},", metrics.sys_write_avg
                )
                .unwrap();
            }
            Err(e) => error!("process exited with error {}", e),
        }
        Ok(())
    }

    fn monitor_energy_consumption(&self, stop: &AtomicBool) -> HashMap<String, Vec<String>> {
        let mut measures: HashMap<String, Vec<String>> = HashMap::new();
        loop {
            if stop.load(Ordering::Relaxed) {
                break;
            }
            let timestamp = SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            for (name, rapl_path) in &self.rapl_paths {
                // get measurement
                let energy_uj = fs::read_to_string(rapl_path).unwrap().trim().to_string();
                measures
                    .entry(name.to_owned())
                    .or_default()
                    .push(format!("{},{}", timestamp, energy_uj));
            }

            thread::sleep(self.energy_sample_interval);
        }

        measures

        // write measures on file
    }

    #[tracing::instrument(level = "trace", skip(self, child))]
    fn collect_metrics(self: Arc<Self>, child: Child, is_sgx: bool) -> Metrics {
        let stop = AtomicBool::new(false);
        let stop = Arc::new(stop);
        let pid = child.id();

        let me = self.clone();
        let energy_stop = stop.clone();
        let energy_handle = thread::spawn(move || me.monitor_energy_consumption(&energy_stop));

        let tracing_stop = stop.clone();
        let me = self.clone();
        let tracing_handle = thread::spawn(move || {
            let skel_builder = TracerSkelBuilder::default();
            let mut open_object = MaybeUninit::uninit();
            let open_skel = skel_builder
                .open(&mut open_object)
                .expect("cannot open ebpf program");
            open_skel.maps.rodata_data.targ_pid = pid as i32;
            trace!(
                "attaching ebpf program on target process with pid {}",
                pid as i32
            );
            let mut prog = open_skel.load().expect("cannot load ebpf program");
            prog.attach().expect("cannot attach program");

            // wait for program to stop
            loop {
                if tracing_stop.load(Ordering::Relaxed) {
                    break;
                }

                thread::sleep(Duration::from_secs(1));
            }

            let mem_counters = get_map_result::<u32, io_counter>(
                &prog.maps.agg_map,
                Some(&|key, value| {
                    let average = if value.count > 0 {
                        value.total_duration / value.count
                    } else {
                        0
                    };

                    trace!(
                        "got {} {} operations; average duration {}ns",
                        value.count,
                        if *key == 0 { "write" } else { "read" },
                        average
                    );
                }),
            )
            .expect("cannot get read/write counters");

            let disk_counters = get_map_result::<u32, disk_counter>(
                &prog.maps.counters,
                Some(&|key, value| {
                    let total = value.sequential + value.random;

                    let mut partition_name = String::from("unknown");
                    for partition in &me.partitions {
                        if partition.dev == *key {
                            partition_name = partition.name.clone();
                        }
                    }

                    trace!(
                        "dev={} random={}% seq={}% total={} bytes={}",
                        partition_name,
                        value.random * 100 / total,
                        value.sequential * 100 / total,
                        total,
                        value.bytes / 1024
                    );
                }),
            )
            .expect("cannot get read/write counters");
            (mem_counters, disk_counters)
        });

        let wait_child_handle = thread::spawn(move || {
            let mut sgx_counters: Option<SGXStats> = None;
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
                                .map_or_else(|| "unknown".to_string(), |c| c.to_string())
                        );
                    }
                    // capture the stderr and get metrics if on sgx
                    if is_sgx {
                        let mut counters = SGXStats::default();
                        for line in output.stderr.lines().flatten() {
                            // # of EENTERs:        139328
                            // # of EEXITs:         139250
                            // # of AEXs:           5377
                            // # of sync signals:   72
                            // # of async signals:  0
                            if line.trim().starts_with("#") {
                                let parts = line.as_str().split_whitespace().collect::<Vec<&str>>();
                                // assert_eq!(parts.len(), 4, "Expected \" # of <metric>: <num>\"");

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

                        sgx_counters = Some(counters);
                    }
                    stderr = output.stderr;
                    stdout = output.stdout;
                }
                Err(e) => error!("target program exited with error {e}"),
            }
            // signal other threads to stop
            stop.store(true, std::sync::atomic::Ordering::Relaxed);
            (stdout, stderr, sgx_counters)
        });

        let (mem_stats, disk_stats) = tracing_handle.join().unwrap();
        let (stdout, stderr, sgx_stats) = wait_child_handle.join().unwrap();
        let energy_stats = energy_handle.join().unwrap();

        let disk_stats = disk_stats
            .iter()
            .map(|c| DiskStats {
                name: "".to_string(),
                bytes: 0,
                perc_random: 100,
                perc_seq: 100,
            })
            .collect::<Vec<DiskStats>>();

        let read_stats = mem_stats[1].1;
        let sys_read_count = read_stats.count;
        let sys_read_avg = if read_stats.count > 0 {
            read_stats.total_duration / read_stats.count
        } else {
            0
        };
        let write_stats = mem_stats[0].1;
        let sys_write_count = write_stats.count;
        let sys_write_avg = if write_stats.count > 0 {
            write_stats.total_duration / write_stats.count
        } else {
            0
        };
        Metrics {
            stdout,
            stderr,
            energy_stats,
            disk_stats,
            sgx_stats,
            sys_read_avg,
            sys_write_avg,
            sys_read_count,
            sys_write_count,
        }
    }

    #[tracing::instrument(level = "debug", skip(self))]
    #[allow(clippy::too_many_arguments)]
    pub fn attach(
        self: Arc<Self>,
        program: PathBuf,
        args: Vec<String>,
        pre_run_executable: Option<PathBuf>,
        pre_run_args: Vec<String>,
        post_run_executable: Option<PathBuf>,
        post_run_args: Vec<String>,
        output_directory: &Path,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let me = self.clone();
        for n in 1..me.clone().sample_size + 1 {
            let experiment_directory = output_directory.join(PathBuf::from(n.to_string()));
            create_dir_all(&experiment_directory)?;

            if let Some(cmd) = &pre_run_executable {
                match Command::new(cmd)
                    .args(pre_run_args.clone())
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .status()?
                    .code()
                    .unwrap()
                {
                    0 => {}
                    n => warn!(
                        "pre exec command {:?} exited with status {}",
                        n,
                        cmd.to_str().unwrap()
                    ),
                };
            }

            me.clone()
                .run_experiment(&program, &args, experiment_directory.as_path())?;

            if let Some(cmd) = &post_run_executable {
                match Command::new(cmd)
                    .args(post_run_args.clone())
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .status()?
                    .code()
                    .unwrap()
                {
                    0 => {}
                    n => warn!(
                        "post exec command {:?} exited with status {}",
                        n,
                        cmd.to_str().unwrap()
                    ),
                };
            }
        }
        Ok(())
    }
}

fn extract_rapl_path(entry: &DirEntry) -> Option<(String, PathBuf)> {
    if entry
        .file_name()
        .to_str()
        .unwrap()
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
fn get_map_result<K: Plain + Clone, T: Plain + Default>(
    map: &Map,
    cb: Option<&dyn Fn(&K, &T)>,
) -> Result<Vec<(K, T)>, Error> {
    let mut result = Vec::new();
    for key in map.keys() {
        let value = map
            .lookup(&key, MapFlags::ANY)
            .expect("cannot read from aggregated map");

        if let Some(bytes) = value {
            let mut value = T::default();
            let key = K::from_bytes(&key).expect("cannot convert map key");
            plain::copy_from_bytes(&mut value, &bytes)?;

            if let Some(cb) = cb {
                cb(key, &value);
            }
            result.push((key.clone(), value));
        }
    }
    Ok(result)
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

#[cfg(test)]
mod test {
    use std::{path::PathBuf, sync::Arc, time::Duration};

    use tempfile::TempDir;

    use super::{DefaultCollector, Partition};

    #[test]
    fn test_collector() {
        let output_directory = TempDir::new().unwrap();
        let sample_size = 1;
        let collector = DefaultCollector::new(sample_size, Duration::from_micros(500), None);
        let collector = Arc::new(collector);
        collector
            .clone()
            .attach(
                PathBuf::from("/bin/sleep"),
                vec!["1".to_string()],
                None,
                vec![],
                None,
                vec![],
                output_directory.path(),
            )
            .unwrap();

        for i in 1..sample_size + 1 {
            assert!(output_directory
                .path()
                .join(format!("{}/perf.csv", i))
                .is_file());
            for (name, _) in &collector.rapl_paths {
                assert!(output_directory
                    .path()
                    .join(format!("{}/{}.csv", i, name))
                    .is_file())
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
