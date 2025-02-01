use std::{
    collections::{HashMap, HashSet},
    fmt::Debug,
    fs::{self, create_dir_all, DirEntry, File},
    mem::MaybeUninit,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{Arc, Mutex},
    thread,
    time::{Duration, SystemTime},
};

use crossbeam::channel::{unbounded, TryRecvError};
use duration_str::HumanFormat;
use libbpf_rs::{
    skel::{OpenSkel, Skel, SkelBuilder},
    PerfBufferBuilder, RingBufferBuilder,
};
use plain::Plain;
use tracer::TracerSkelBuilder;
use tracing::{error, trace, warn};

mod tracer {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/src/bpf/tracer.skel.rs"
    ));
}

pub trait Collector {
    #[allow(clippy::too_many_arguments)]
    fn attach(
        &self,
        _program: PathBuf,
        _args: Vec<String>,
        _pre_run_executable: Option<PathBuf>,
        _pre_run_args: Vec<String>,
        _post_run_executable: Option<PathBuf>,
        _post_run_args: Vec<String>,
        _output_directory: &Path,
    ) -> Result<(), Box<dyn std::error::Error>>;
}

pub struct DefaultLinuxCollector {
    sample_size: u32,
    perf_events: Vec<String>,
    rapl_paths: Vec<(String, PathBuf)>,
    energy_sample_interval: Duration,
}

impl DefaultLinuxCollector {
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
        &self,
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
                self.monitor_child_process(child, experiment_directory.to_path_buf());
            }
            Err(e) => panic!("process exited with error {}", e),
        }
        Ok(())
    }

    #[tracing::instrument(level = "trace", skip(self))]
    fn monitor_child_process(&self, child: Child, experiment_directory: PathBuf) {
        let (tx, rx) = unbounded::<(u64, u64)>();
        let rx1 = rx.clone();
        let pid = child.id();
        let child = Arc::new(Mutex::new(child));

        let rapl_paths = self.rapl_paths.clone();
        let interval = self.energy_sample_interval;
        let energy_handle = thread::spawn(move || {
            let mut measures: HashMap<String, Vec<String>> = HashMap::new();
            loop {
                if let Err(e) = rx.try_recv() {
                    if e == TryRecvError::Disconnected {
                        trace!("got termination signal {}", e);
                        break;
                    }
                }
                let timestamp = SystemTime::now()
                    .duration_since(SystemTime::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos();
                for (name, rapl_path) in &rapl_paths {
                    // get measurement
                    let energy_uj = fs::read_to_string(rapl_path).unwrap().trim().to_string();
                    measures
                        .entry(name.to_owned())
                        .or_default()
                        .push(format!("{},{}", timestamp, energy_uj));
                }

                thread::sleep(interval);
            }

            // write measures on file
            for (filename, data) in measures.iter_mut() {
                data.insert(0, Self::ENERGY_CSV_HEADER.to_owned());
                fs::write(
                    experiment_directory.join(format!("{}.csv", filename)),
                    data.join("\n"),
                )
                .unwrap();
            }
        });

        let tracing_handle = thread::spawn(move || {
            let mut skel_builder = TracerSkelBuilder::default();
            skel_builder.obj_builder.debug(true);
            let mut open_object = MaybeUninit::uninit();
            let open_skel = skel_builder
                .open(&mut open_object)
                .expect("cannot open ebpf program");
            open_skel.maps.rodata_data.targ_pid = pid as i32;
            let mut skel = open_skel.load().expect("cannot load ebpf program");
            skel.attach().expect("cannot attach program");

            let mut ring_buffer_builder = RingBufferBuilder::new();
            ring_buffer_builder
                .add(&skel.maps.ringbuf, move |data| -> i32 {
                    trace!("got into callback");
                    let mut event = tracer::types::exec_event::default();
                    plain::copy_from_bytes(&mut event, data).expect("Data buffer was too short");

                    let a = event.filename.map(|c| c as u8);
                    let filename = std::str::from_utf8(&a).unwrap();

                    // Process the event (e.g., log or write to CSV).
                    trace!("event: syscall at {} ns ({})", event.timestamp, filename,);
                    0
                })
                .expect("cannot add map to ringbuf");
            let ring_buffer = &ring_buffer_builder.build().expect("cannot build ringbuf");

            loop {
                if let Err(e) = rx1.try_recv() {
                    if e == TryRecvError::Disconnected {
                        trace!("ebpf: got termination signal {}", e);
                        break;
                    }
                }
                if let Err(e) = ring_buffer.consume() {
                    warn!("ebpf ring_buffer.consume {e}");
                }

                thread::sleep(Duration::from_millis(250));
            }
        });

        let child = child.clone();
        let wait_child_handle = thread::spawn(move || {
            let status = {
                let mut child_guard = child.lock().unwrap();
                child_guard.wait()
            };
            match status {
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
            drop(tx);
        });

        for handle in [energy_handle, wait_child_handle, tracing_handle] {
            handle.join().unwrap();
        }
    }
}

impl Collector for DefaultLinuxCollector {
    #[tracing::instrument(level = "debug", skip(self))]
    fn attach(
        &self,
        program: PathBuf,
        args: Vec<String>,
        pre_run_executable: Option<PathBuf>,
        pre_run_args: Vec<String>,
        post_run_executable: Option<PathBuf>,
        post_run_args: Vec<String>,
        output_directory: &Path,
    ) -> Result<(), Box<dyn std::error::Error>> {
        for n in 1..self.sample_size + 1 {
            let experiment_directory = output_directory.join(PathBuf::from(n.to_string()));
            create_dir_all(&experiment_directory)?;

            // i have no access to sgx machine yet
            if program.to_str().unwrap() == "gramine-sgx" {
                File::create(experiment_directory.join("perf.csv"))?;
                for (name, _) in &self.rapl_paths {
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

            self.run_experiment(&program, &args, &experiment_directory)?;

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

unsafe impl Plain for tracer::types::exec_event {}

impl Debug for dyn Collector {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Collector debug")
    }
}

impl Debug for DefaultLinuxCollector {
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

#[cfg(test)]
mod test {
    use std::{path::PathBuf, time::Duration};

    use tempfile::TempDir;

    use super::{Collector, DefaultLinuxCollector};

    #[test]
    fn test_collector() {
        let output_directory = TempDir::new().unwrap();
        let sample_size = 1;
        let collector = DefaultLinuxCollector::new(sample_size, Duration::from_micros(500), None);
        collector
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
}
