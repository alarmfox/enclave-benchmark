import marimo

__generated_with = "0.11.8"
app = marimo.App()


@app.cell
def _(__file__):
    import marimo as mo
    import matplotlib.pyplot as plt
    import pandas as pd

    import os
    import toml

    project_root = os.path.abspath(os.path.join(os.path.dirname(__file__), os.path.pardir))
    print(project_root)
    os.chdir(project_root)

    config = "demo-result/config.toml"

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
    def _():
        for thread in data["sysbench"]["num_threads"]:
            fname = f"aggregated/sgx-sysbench-{thread}-1G-untrusted/package-0.csv"
            df = pd.read_csv(fname)
            plt.plot(df["relative_time"] /1e9, df["energy (microjoule)"] / 1e6, label=f"{thread}")

        plt.title("Sysbench energy by number of threads")
        plt.xlabel("Time (s)")
        return plt.ylabel("Energy (J)")


    _()
    return


@app.cell
def _(cmp_perf_param, data):
    cmp_perf_param("sysbench", "cache-misses", data["sysbench"]["num_threads"], "1G")
    cmp_perf_param("sysbench", "cpu-cycles", data["sysbench"]["num_threads"], "1G")
    cmp_perf_param("sysbench", "cache-references", data["sysbench"]["num_threads"], "1G")
    cmp_perf_param("sysbench", "branch-instructions", data["sysbench"]["num_threads"], "1G")
    cmp_perf_param("sysbench", "duration_time", data["sysbench"]["num_threads"], "1G")
    cmp_perf_param("sysbench", "branch-loads", data["sysbench"]["num_threads"], "1G")
    cmp_perf_param("sysbench", "branch-misses", data["sysbench"]["num_threads"], "1G")
    return


@app.cell
def _(pd, plt):
    def cmp_perf_param(app: str, param: str, threads: list[int], size: str):
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
        plt.xticks(ticks, labels, rotation=45,ha='right')
        plt.legend()
        plt.show()
    return (cmp_perf_param,)


@app.cell
def _(pd):
    pd.read_csv("aggregated/sgx-sysbench-1-1G-untrusted/perf.csv")
    return


@app.cell
def _(data, deep_trace_analysis):
    deep_trace_analysis("dd", 1, "128M", "encrypted")
    deep_trace_analysis("dd", 1, "128M", "untrusted")
    deep_trace_analysis("dd", 1)

    for thread in data["sysbench"]["num_threads"]:
        deep_trace_analysis("sysbench", thread, "1G", "untrusted")
        deep_trace_analysis("sysbench", thread)
    return (thread,)


@app.cell
def _(pd, plt):
    def deep_trace_analysis(app: str, threads: int, enclave: str = None, storage: str = None ):
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
        title = f"{app}-{enclave} w/ {threads} thr {storage}" if sgx_suffix != "" else f"{app} w/out sgx"
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


        plt.show()
    return (deep_trace_analysis,)


@app.cell
def _():
    return


if __name__ == "__main__":
    app.run()
