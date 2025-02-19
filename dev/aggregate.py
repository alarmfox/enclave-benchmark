import toml
import sys
import os
import re

import pandas as pd
import numpy as np

from typing import List

if len(sys.argv) != 3:
    print("Usage: python analysis/pre-process.py </path/to/toml> </path/to/output_directory>")
    sys.exit(1)

SKIP_SGX = os.environ.get("EB_SKIP_SGX", False)

W = 10 # microseconds

input_file, output_directory = sys.argv[1], sys.argv[2]
print("Reading from", input_file)
with open(input_file, 'r') as f:
    config = toml.load(f)

os.makedirs(output_directory, exist_ok=True)

print("Created output directory", output_directory)

n = config["globals"]["sample_size"]
num_threads = config["globals"]["num_threads"]
enclave_size = config["globals"]["enclave_size"]
input_directory = config["globals"]["output_directory"]
deep_trace = config["globals"]["deep_trace"]
tasks = [(os.path.basename(t["executable"]), t.get("storage_type", ["untrusted"]) ) for t in config["tasks"]]

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

def process_energy_samples(files: List[str]) -> pd.DataFrame:
    """
    Processes energy sample files to calculate the average energy consumption over time bins.

    This function reads multiple CSV files containing energy data, calculates the relative time for each 
    energy measurement, and bins the data into intervals of width W. It then computes the mean relative 
    time and mean energy consumption for each bin across all files.

    Parameters:
    files (List[str]): A list of file paths to the CSV files containing energy data. Each file is expected 
                       to have columns: "timestamp (us)" and "energy (microjoule)".

    Returns:
    pd.DataFrame: A DataFrame containing the binned energy data with the following columns:
                  - 'bin': The bin index, representing intervals of width W.
                  - 'relative_time': The mean relative time for each bin.
                  - 'energy (microjoule)': The mean energy consumption for each bin.
    """
    all_binned = []

    for filename in files:
        df = pd.read_csv(filename)
        
        df['timestamp (us)'] = df['timestamp (us)'].astype(np.int64)
        
        df['relative_time'] = df['timestamp (us)'] - df['timestamp (us)'].iloc[0]
        
        df['bin'] = (df['relative_time'] // W).astype(int)
        
        binned = df.groupby('bin').agg({
            'relative_time': 'mean',  
            'energy (microjoule)': 'mean'
        }).reset_index()
        
        all_binned.append(binned)
    
    combined = pd.concat(all_binned, ignore_index=True)
    avg_binned = combined.groupby('bin').agg({
        'relative_time': 'mean',
        'energy (microjoule)': 'mean'
    }).reset_index()

    return avg_binned

def process_io(files: List[str]) -> pd.DataFrame:

    
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

first_prog = tasks[0][0]
first_exp = f"{first_prog}/no-gramine-sgx/{first_prog}-{num_threads[0]}/{first_prog}-{num_threads[0]}-untrusted/1"
energy_files = get_energy_files(os.path.join(input_directory, first_exp))

print("Discovered following energy sample files", energy_files)

# non gramine sgx
for (task, storages) in tasks:
    print("Processing", task)
    for thread in num_threads:
        experiment_dir = os.path.join(input_directory, 
                                  task, 
                                  "no-gramine-sgx",
                                  f"{task}-{thread}", 
                                  f"{task}-{thread}-untrusted")
        
        result_directory = os.path.join(output_directory, f"{task}-{thread}-untrusted")
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


if SKIP_SGX:
    print("Skipped SGX parsing")
    sys.exit(0)

# gramine sgx
for (task, storages) in tasks:
    print("Processing", task)
    for thread in num_threads:
        for storage in storages:
            experiment_dir = os.path.join(input_directory, 
                                      task, 
                                      "gramine-sgx",
                                      f"{task}-{thread}", 
                                      f"{task}-{thread}-{storage}")
            
            result_directory = os.path.join(output_directory, f"sgx-{task}-{thread}-{storage}")
            os.makedirs(result_directory, exist_ok=True)

            perf_files = [os.path.join(experiment_dir, f"{i}/perf.csv") for i in range(1, n+1)]
            df = process_perf_samples(perf_files)
            df.to_csv(os.path.join(result_directory, f"perf.csv"), index=False)

            io_files = [os.path.join(experiment_dir, f"{i}/io.csv") for i in range(1, n+1)]
            df = process_io(io_files)
            df.to_csv(os.path.join(result_directory, "io.csv"), index=False)

            for file in energy_files:
                files = [os.path.join(experiment_dir, f"{i}/{file}") for i in range(1, n+1)]
                avg = process_energy_samples(files)
                avg.to_csv(os.path.join(result_directory, file))
