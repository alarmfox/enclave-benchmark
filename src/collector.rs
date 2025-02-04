use std::{
    collections::{HashMap, HashSet},
    fmt::Debug,
    fs::{self, create_dir_all, DirEntry, File},
    io::{BufRead, BufReader},
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
    Map, MapCore, MapFlags,
};
use plain::{Error, Plain};
use tracer::{
    types::{disk_counter, io_counter},
    TracerSkelBuilder,
};
use tracing::{error, trace, warn};

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

impl DefaultCollector {
    const ENERGY_CSV_HEADER: &str = "timestamp (microseconds),energy (microjoule)";
    const DEFAULT_PERF_EVENTS: [&str; 28] = [
        "user_time",
        "system_time",
        "duration_time",
        "cycles",
        "instructions",
        "cache-misses",
        "L1-dcache-loads",
        "L1-dcache-load-misses",
        "L1-dcache-prefetches",
        "L1-icache-loads",
        "L1-icache-load-misses",
        "dTLB-loads",
        "dTLB-load-misses",
        "iTLB-loads",
        "iTLB-load-misses",
        "branch-loads",
        "branch-load-misses",
        "branch-instructions",
        "branch-misses",
        "cache-misses",
        "cache-references",
        "cpu-cycles",
        "instructions",
        "stalled-cycles-frontend",
        "branch-misses",
        "cache-misses",
        "cpu-cycles",
        "page-faults",
    ];
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
                    HashSet::from_iter(Self::DEFAULT_PERF_EVENTS.iter().map(|v| v.to_string()));
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
        let cmd = {
            Command::new(program)
                .args(args)
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
        };
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

                // Monitor the child process
                self.monitor_child_process(child, &experiment_directory);
            }
            Err(e) => error!("process exited with error {}", e),
        }
        Ok(())
    }

    fn monitor_energy_consumption(&self, stop: &AtomicBool, out_directory: &Path) {
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

        // write measures on file
        for (filename, data) in measures.iter_mut() {
            data.insert(0, Self::ENERGY_CSV_HEADER.to_owned());
            fs::write(
                out_directory.join(format!("{}.csv", filename)),
                data.join("\n"),
            )
            .unwrap();
        }
    }

    #[tracing::instrument(level = "trace", skip(self, child, experiment_directory))]
    fn monitor_child_process(self: &Arc<Self>, child: Child, experiment_directory: &Path) {
        let stop = AtomicBool::new(false);
        let stop = Arc::new(stop);
        let pid = child.id();
        let child = Mutex::new(child);

        let energy_result_directory = experiment_directory.to_path_buf().clone();
        let me = self.clone();
        let energy_stop = stop.clone();
        let energy_handle = thread::spawn(move || {
            me.monitor_energy_consumption(&energy_stop, &energy_result_directory);
        });

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

            let _io_counters = get_map_result::<u32, io_counter>(
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
            let _disk_counters = get_map_result::<u32, disk_counter>(
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
        });

        let wait_child_handle = thread::spawn(move || {
            match child.lock().unwrap().wait() {
                Ok(status) => match status.code() {
                    Some(n) => {
                        if n != 0 {
                            warn!("target program exited with non-zero code {}", n);
                        }
                    }
                    None => warn!("cannot get exit status code for target program"),
                },
                Err(e) => error!("target program exited with error {e}"),
            }
            // signal other threads to stop
            stop.store(true, std::sync::atomic::Ordering::Relaxed);
        });

        for handle in [energy_handle, wait_child_handle, tracing_handle] {
            handle.join().unwrap();
        }
    }

    #[tracing::instrument(level = "debug", skip(self))]
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
        for n in 1..self.clone().sample_size + 1 {
            let experiment_directory = output_directory.join(PathBuf::from(n.to_string()));
            create_dir_all(&experiment_directory)?;

            // i have no access to sgx machine yet
            if program.to_str().unwrap() == "gramine-sgx" {
                File::create(experiment_directory.join("perf.csv"))?;
                for (name, _) in &self.clone().rapl_paths {
                    File::create(experiment_directory.join(format!("{}.csv", name)))?;
                }
                continue;
            }

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

            self.clone()
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
                cb(&key, &value);
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
