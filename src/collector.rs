use std::{
    collections::{HashMap, HashSet},
    fmt::Debug,
    fs::{self, create_dir_all, DirEntry, File},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{Arc, Mutex},
    thread,
    time::{Duration, SystemTime},
};

use crossbeam::channel::{unbounded, TryRecvError};
use tracing::{error, trace, warn};

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
                        match Self::extract_rapl_path(&entry) {
                            //found a path like /sys/devices/virtual/powercap/intel-rapl/intel-rapl:<num>/
                            Some(s) => {
                                let domain_name = s.0.clone();
                                rapl_paths.push(s);
                                for subentry in entry.path().read_dir().unwrap().flatten() {
                                    // /sys/devices/virtual/powercap/intel-rapl/intel-rapl:<num>/intel-rapl:<num>
                                    if let Some(r) = Self::extract_rapl_path(&subentry) {
                                        let name = format!("{}-{}", domain_name, r.0);
                                        rapl_paths.push((name, r.1));
                                    };
                                }
                            }
                            None => continue,
                        };
                    }
                } else {
                    warn!("apparently system does not support RAPL interface; skipping");
                }
                rapl_paths
            },
        }
    }

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

    fn monitor_child_process(&self, child: Child, experiment_directory: PathBuf) {
        let (tx, rx) = unbounded::<(u64, u64)>();
        let rx1 = rx.clone();
        let child = Arc::new(Mutex::new(child));

        let rapl_paths = self.rapl_paths.clone();
        let interval = self.energy_sample_interval;
        let energy_handle = thread::spawn(move || {
            // setup variables
            let mut measures: HashMap<String, Vec<String>> = HashMap::new();
            loop {
                if let Err(e) = rx1.try_recv() {
                    if e == TryRecvError::Disconnected {
                        trace!("got termination {}", e);
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

        for handle in [energy_handle, wait_child_handle] {
            handle.join().unwrap();
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
}

impl Collector for DefaultLinuxCollector {
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
                File::create(experiment_directory.join("ptrace.log"))?;
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

impl Debug for dyn Collector {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Collector debug")
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
