import toml
import sys
import os
import re
import shutil

import pandas as pd
import numpy as np

from typing import List, Union

if len(sys.argv) != 3:
    print("Usage: python analysis/pre-process.py </path/to/toml> </path/to/output_directory>")
    sys.exit(1)

SKIP_SGX = os.environ.get("EB_SKIP_SGX", False)

input_file, output_directory = sys.argv[1], sys.argv[2]
print("Reading from", input_file)
with open(input_file, 'r') as f:
    config = toml.load(f)

os.makedirs(output_directory, exist_ok=True)

print("Created output directory", output_directory)

n = config["globals"]["sample_size"]
input_directory = config["globals"]["output_directory"]
deep_trace = config["globals"]["deep_trace"]
tasks = config["tasks"]

# Calculate the avg and std-dev for perf perf samples
def process_perf_samples(files: List[str]) -> pd.DataFrame:
    """
    Processes performance sample files to calculate the average and standard deviation of various metrics.

    This function reads multiple CSV files containing performance data, concatenates them into a single DataFrame,
    and then groups the data by the 'event' column. It calculates the mean and standard deviation for the 'counter'
    column, as well as the mean for the 'metric' and 'perc_runtime' columns. The first value of 'unit_counter' and
    'unit_metric' is retained for each group.

    Parameters:
    files (List[str]): A list of file paths to the CSV files containing performance data. Each file is expected to
                       have columns: "counter", "unit_counter", "event", "runtime_counter", "perc_runtime", "metric",
                       and "unit_metric".

    Returns:
    pd.DataFrame: A DataFrame containing the aggregated performance data with the following columns:
                  - 'event': The event name.
                  - 'counter_mean': The mean of the 'counter' values for each event.
                  - 'counter_std': The standard deviation of the 'counter' values for each event.
                  - 'counter_unit': The unit of the counter, taken from the first occurrence in each group.
                  - 'metric_mean': The mean of the 'metric' values for each event.
                  - 'unit_metric': The unit of the metric, taken from the first occurrence in each group.
                  - 'perc_runtime_mean': The mean of the 'perc_runtime' values for each event.
    """

    df = pd.concat([pd.read_csv(f, names=["counter", 
                                          "unit_counter", 
                                          "event", 
                                          "runtime_counter", 
                                          "perc_runtime", 
                                          "metric", 
                                          "unit_metric"]) 
                    for f in files])

    df_new = df.groupby("event").agg(
        counter_mean=("counter", "mean"),
        counter_std=("counter", "std"),
        counter_unit=("unit_counter", "first"),
        metric_mean=("metric", "mean"),
        unit_metric=("unit_metric", "first"),
        perc_runtime_mean=("perc_runtime", "mean")
    ).reset_index()

    return df_new

def process_energy_samples(files: list, W: int = 10_000):  # W in microseconds (default: 10ms)
    """
    Processes energy sample files to calculate the average energy consumption over a common time grid.

    Parameters:
    files (List[str]): List of file paths to the CSV files containing energy data.
    W (int): Coalescence window width in microseconds.

    Returns:
    pd.DataFrame: DataFrame containing the averaged energy data at uniform time intervals with:
                  - 'relative_time': Time bins (0, W, 2W, ...)
                  - 'energy (microjoule)': Mean energy across all files at each time bin.
    """
    common_time_grid = None
    interpolated_energies = []

    for filename in files:
        df = pd.read_csv(filename)
        df['relative_time'] = df['timestamp (ns)'] - df['timestamp (ns)'].iloc[0]

        if common_time_grid is None:
            max_time = df['relative_time'].max()
            common_time_grid = np.arange(0, max_time + W, W)

        interp_energy = np.interp(common_time_grid, df['relative_time'], df['energy (microjoule)'])
        interpolated_energies.append(interp_energy)

    energy_matrix = np.vstack(interpolated_energies)
    avg_energy = np.mean(energy_matrix, axis=0)

    return pd.DataFrame({'relative_time': common_time_grid, 'energy (microjoule)': avg_energy})

def process_io(files: List[str]) -> pd.DataFrame:
    """
    Processes I/O sample files to calculate the average value for each dimension and description.

    This function reads multiple CSV files containing I/O data, concatenates them into a single DataFrame,
    and then groups the data by the 'dimension' and 'description' columns. It calculates the mean of the 
    'value' column and retains the first occurrence of the 'unit' for each group.

    Parameters:
    files (List[str]): A list of file paths to the CSV files containing I/O data. Each file is expected to
                       have columns: "dimension", "description", "value", and "unit".

    Returns:
    pd.DataFrame: A DataFrame containing the aggregated I/O data with the following columns:
                  - 'dimension': The dimension name.
                  - 'description': The description of the dimension.
                  - 'value_mean': The mean of the 'value' for each dimension and description.
                  - 'value_unit': The unit of the value, taken from the first occurrence in each group.
    """
    df = pd.concat([pd.read_csv(f) for f in files])
    df_new = df.groupby(["dimension", "description"]).agg(
        value_mean=("value", "mean"),
        value_unit=("unit", "first"),
    ).reset_index()

    return df_new

def get_energy_files(samples_directory: str) -> List[str]:
    """
    Scans a directory for energy sample files and returns a list of matching filenames.

    This function searches through the specified directory for files that match a specific naming pattern
    related to energy samples. The pattern is defined by the regular expression 'package-\\d+(-core|-uncore|-dram)?\\.csv',
    which matches filenames like 'package-1.csv', 'package-2-core.csv', 'package-3-uncore.csv', etc.

    Parameters:
    samples_directory (str): The path to the directory containing the energy sample files.

    Returns:
    List[str]: A list of filenames that match the energy sample pattern.
    """
    files = []
    for file in os.listdir(samples_directory):
        fname = os.path.basename(file)
        match = re.match(r'package-\d+(-core|-uncore|-dram)?\.csv', fname)
        if match:
            files.append(fname)
    return files


# Function to process experiments
def process_experiment(task: str, thread: int, storage: Union[str, None] = None, sgx: bool = False)-> None:
    """
    Processes experimental data for a given task and thread configuration, optionally considering storage type and SGX usage.

    This function organizes and processes performance, I/O, and energy data for a specific experimental setup. It reads data from
    CSV files located in a structured directory hierarchy, processes the data using helper functions, and writes the aggregated
    results to an output directory. If deep tracing is enabled, it also copies the deep trace data to the output directory.

    Parameters:
    task (str): The name of the task or executable being analyzed.
    thread (int): The number of threads used in the experiment.
    storage (str, optional): The type of storage used in the experiment. Defaults to None, which implies "untrusted" storage.
    sgx (bool, optional): A flag indicating whether the experiment was run with SGX (Software Guard Extensions). Defaults to False.

    Returns:
    None: This function does not return a value. It writes the processed data to CSV files in the specified output directory.
    """
    sgx_prefix = "sgx-" if sgx else ""
    storage_suffix = f"-{storage}" if storage else "-untrusted"
    experiment_type = "gramine-sgx" if sgx else "no-gramine-sgx"
    
    experiment_dir = os.path.join(input_directory, 
                                  task, 
                                  experiment_type,
                                  f"{task}-{thread}", 
                                  f"{task}-{thread}{storage_suffix}")
    
    result_directory = os.path.join(output_directory, f"{sgx_prefix}{task}-{thread}{storage_suffix}")
    os.makedirs(result_directory, exist_ok=True)

    perf_files = [os.path.join(experiment_dir, f"{i}/perf.csv") for i in range(1, n+1)]
    df = process_perf_samples(perf_files)
    df.to_csv(os.path.join(result_directory, "perf.csv"), index=False)

    io_files = [os.path.join(experiment_dir, f"{i}/io.csv") for i in range(1, n+1)]
    df = process_io(io_files)
    df.to_csv(os.path.join(result_directory, "io.csv"), index=False)

    for file in energy_files:
        files = [os.path.join(experiment_dir, f"{i}/{file}") for i in range(1, n+1)]
        avg = process_energy_samples(files)
        avg.to_csv(os.path.join(result_directory, file))

    if deep_trace:
        deep_trace_directory = os.path.join(experiment_dir, "deep-trace")
        shutil.copytree(deep_trace_directory, os.path.join(result_directory, "deep-trace"))

first_prog = os.path.basename(tasks[0]["executable"])
num_threads = tasks[0].get("num_threads", [1])

first_exp = f"{first_prog}/no-gramine-sgx/{first_prog}-{num_threads[0]}/{first_prog}-{num_threads[0]}-untrusted/1"
energy_files = get_energy_files(os.path.join(input_directory, first_exp))

print("Discovered following energy sample files", energy_files)
# Process non-gramine SGX tasks
for task in tasks:
    prog = os.path.basename(task["executable"])
    print("Processing", task, end="... ")
    for thread in task.get("num_threads", [1]):
        process_experiment(prog, thread)
    print("done")

if SKIP_SGX:
    print("Skipped SGX parsing")
    sys.exit(0)

# Process gramine SGX tasks
for task in tasks:
    prog = os.path.basename(task["executable"])
    print("Processing", task, end="... ")
    for thread in task,get("num_threads", [1]):
        for storage in task.get("storage_type", ["untrusted"]):
            process_experiment(prog, thread, storage, sgx=True)
    print("done")
