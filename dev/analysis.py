import marimo

__generated_with = "0.11.8"
app = marimo.App()


@app.cell
def _(__file__):
    import marimo as mo
    import matplotlib.pyplot as plt
    import pandas as pd
    import numpy as np

    import os
    import toml

    project_root = os.path.abspath(os.path.join(os.path.dirname(__file__), os.path.pardir))
    print(project_root)
    os.chdir(project_root)

    config = "data/demo-result2/config.toml"

    # load experiment params
    config = toml.load(config)
    globals = config["globals"]
    deep_trace = globals["deep_trace"]
    tasks = config["tasks"]

    data = {}
    for task in tasks:
        prog = os.path.basename(task["executable"])
        data[prog] = task

    data
    return (
        config,
        data,
        deep_trace,
        globals,
        mo,
        np,
        os,
        pd,
        plt,
        prog,
        project_root,
        task,
        tasks,
        toml,
    )


@app.cell
def _(data, pd, plt):
    def deep_trace_analysis(app: str, threads: int, enclave: str = None, storage: str = None ) -> None:
        size_suffix = f"-{enclave}" if enclave is not None else ""
        storage_suffix = f"-{storage}" if storage is not None else ""
        sgx_suffix = "sgx-" if enclave is not None else ""
        df = pd.read_csv(f"aggregated/{sgx_suffix}{app}-{threads}{size_suffix}{storage_suffix}/deep-trace/trace.csv")

        df['relative_time'] = (df['timestamp (ns)'] - df['timestamp (ns)'].min()) /1e9

        event_colors = {
            "sys-read":     {"color": "blue",   "alpha": 0.5},
            "sys-write":    {"color": "blue",   "alpha": 0.8},
            "mm-page-alloc": {"color": "green",  "alpha": 0.5},
            "mm-page-free":  {"color": "green",  "alpha": 0.8},
            "kmalloc":      {"color": "orange", "alpha": 0.5},
            "kfree":        {"color": "orange", "alpha": 0.8},
            "disk-read":    {"color": "red",    "alpha": 0.5},
            "disk-write":   {"color": "red",    "alpha": 0.8},
        }

        system_events = ["sys-read", "sys-write", "disk-read", "disk-write"]
        memory_events = ["mm-page-alloc", "mm-page-free", "kmalloc", "kfree"]

        fig, (ax_sys, ax_mem) = plt.subplots(2, 1, sharex=True, figsize=(12, 10))
        title = f"{app}-{enclave} w/ {threads} threads {storage}" if sgx_suffix != "" else f"{app} {threads} threads w/out sgx"
        fig.suptitle(title)
        bins = 100

        for event in system_events:
            event_data = df[df['event'] == event]
            ax_sys.hist(event_data['relative_time'], bins=bins,
                        color=event_colors[event]["color"],
                        alpha=event_colors[event]["alpha"],
                        label=event)
        ax_sys.set_title("System and Disk Events")
        ax_sys.set_ylabel("Count")
        ax_sys.legend()

        for event in memory_events:
            event_data = df[df['event'] == event]
            ax_mem.hist(event_data['relative_time'], bins=bins,
                        color=event_colors[event]["color"],
                        alpha=event_colors[event]["alpha"],
                        label=event)
        ax_mem.set_title("Memory Allocation/Free Events")
        ax_mem.set_xlabel("Relative Time (ns)")
        ax_mem.set_ylabel("Count")
        ax_mem.legend()

        fname = f"{app}-{enclave}-{threads}-{storage}" if sgx_suffix != "" else f"{app}-{threads}-no-sgx"
        plt.savefig(f"figures/{fname}.png")
        plt.show()

    deep_trace_analysis("dd", 1, "128M", "encrypted")
    deep_trace_analysis("dd", 1, "128M", "untrusted")
    deep_trace_analysis("dd", 1)

    for thread in data["sysbench"]["num_threads"]:
        deep_trace_analysis("sysbench", thread, "1G", "untrusted")
        deep_trace_analysis("sysbench", thread)
    return deep_trace_analysis, thread


@app.cell
def _(data, pd, plt):
    def cmp_perf_param(app: str, param: str, threads: list[int], size: str) -> None:
        n = len(threads)
        for (i, thread) in enumerate(threads):
            fname = f"aggregated/sgx-{app}-{thread}-{size}-untrusted/perf.csv"
            df = pd.read_csv(fname, index_col = 0)
            v = df.loc[param]["counter_mean"]
            plt.bar(i, v, label=f"sgx-{thread} thr")

        for (i, thread) in enumerate(threads):
            fname = f"aggregated/{app}-{thread}/perf.csv"
            df = pd.read_csv(fname, index_col = 0)
            v = df.loc[param]["counter_mean"]
            plt.bar(i+n, v, label=f"no-sgx-{thread} thr")

        ticks = list(range(n)) + list(range(n, 2*n))
        labels =  [f"sgx-{t} threads" for t in threads] + [f"no-sgx-{threads[t-n]} threads" for t in range(n, 2*n)]
        plt.title(f"{app} {param}")
        plt.xlabel("Experiment")
        plt.ylabel(param)
        plt.xticks(ticks, labels, rotation=45, ha='right')
        fname = f"{param}-sgx-{app}-{size}"
        plt.tight_layout()
        plt.savefig(f"figures/{fname}.png")
        plt.legend()
        plt.show()

    cmp_perf_param("sysbench", "cache-misses", data["sysbench"]["num_threads"], "1G")
    cmp_perf_param("sysbench", "cpu-cycles", data["sysbench"]["num_threads"], "1G")
    cmp_perf_param("sysbench", "cache-references", data["sysbench"]["num_threads"], "1G")
    cmp_perf_param("sysbench", "branch-instructions", data["sysbench"]["num_threads"], "1G")
    cmp_perf_param("sysbench", "duration_time", data["sysbench"]["num_threads"], "1G")
    cmp_perf_param("sysbench", "branch-loads", data["sysbench"]["num_threads"], "1G")
    cmp_perf_param("sysbench", "branch-misses", data["sysbench"]["num_threads"], "1G")
    return (cmp_perf_param,)


@app.cell
def _(np, pd, plt):
    def cmp_disk_write(app: str, threads: int, enclave: str) -> None:
        experiments = ["encrypted", "untrusted"]
        seq_vals = []
        rnd_vals = []

        for s in experiments:
            fname = f"aggregated/sgx-{app}-{threads}-{enclave}-{s}/io.csv"
            df = pd.read_csv(fname, index_col=0)
            seq_vals.append(df.loc["disk_write_seq"]["value_mean"])
            rnd_vals.append(df.loc["disk_write_rand"]["value_mean"])

        indices = np.arange(len(experiments))
        bar_width = 0.35  # width of each bar

        plt.bar(indices - bar_width/2, seq_vals, width=bar_width, label="Sequential Writes (%)")
        plt.bar(indices + bar_width/2, rnd_vals, width=bar_width, label="Random Writes (%)")
        # Set x-axis tick positions and labels
        plt.xticks(indices, experiments, rotation=45, ha='right')
        plt.xlabel("Experiment")
        plt.ylabel("Percent write (%)")
        plt.title("Disk Write Comparison")
        plt.legend()
        fname = f"disk_write-sgx-{app}-{enclave}"
        plt.tight_layout()
        plt.savefig(f"figures/{fname}.png")
        plt.show()

    cmp_disk_write("dd", 1, "128M")
    cmp_disk_write("dd", 1, "64M")
    return (cmp_disk_write,)


@app.cell
def _(data, os, pd, plt):
    def energy_analysis(app: str, threads: int, enclave: str = None, storage: str = None ) -> None:
        size_suffix = f"-{enclave}" if enclave is not None else ""
        storage_suffix = f"-{storage}" if storage is not None else ""
        sgx_suffix = "sgx-" if enclave is not None else ""
        efiles = ["package-0.csv", "package-0-core.csv", "package-0-uncore.csv", "package-0-dram.csv"]

        dir = f"aggregated/{sgx_suffix}{app}-{threads}{size_suffix}{storage_suffix}/"
        files = [os.path.join(dir, "{f}".format(f = file)) for file in efiles]

        event_colors = {
            "0":  {"color": "blue",   "alpha": 0.8 },
            "core": {"color": "blue",   "alpha": 0.8 },
            "uncore": {"color": "green",  "alpha": 0.5 },
            "dram": {"color": "green",  "alpha": 0.8 },
        }

        title = f"{app}-{enclave} w/ {threads} thr {storage}" if sgx_suffix != "" else f"{app} w/out sgx"

        plt.title(title)
        bins = 10000

        for file in files:
            df = pd.read_csv(file)
            event = os.path.basename(file).split("-")[-1]
            event = os.path.splitext(event)[0]
            plt.plot(df['relative_time'] /1e9, df['energy (microjoule)'] / 1e6,
                        color=event_colors[event]["color"],
                        alpha=event_colors[event]["alpha"],
                        label=event)
        plt.title(title)
        plt.ylabel("Energy (J)")
        plt.xlabel("Time (s)")
        plt.legend()
        plt.tight_layout()
        plt.show()

    def plot_energy() -> None:
        for size in data["dd"]["enclave_size"]:
            energy_analysis("dd", 1, size, "encrypted")
            energy_analysis("dd", 1, size, "untrusted")   

        energy_analysis("dd", 1)

        for thread in data["sysbench"]["num_threads"]:
            energy_analysis("sysbench", thread, "1G", "untrusted")
            energy_analysis("sysbench", thread)


    return energy_analysis, plot_energy


@app.cell
def _():
    return


if __name__ == "__main__":
    app.run()
