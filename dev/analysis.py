import marimo

__generated_with = "0.11.7"
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
    data = "aggregated"

    # load experiment params
    config = toml.load(config)
    globals = config["globals"]
    deep_trace = globals["deep_trace"]
    tasks = config["tasks"]

    deep_trace, tasks
    return (
        config,
        data,
        deep_trace,
        globals,
        mo,
        os,
        pd,
        plt,
        project_root,
        tasks,
        toml,
    )


@app.cell
def _(data, os, pd, plt, tasks):
    def _ ():
        prog = os.path.basename(tasks[0]["executable"])

        for thread in tasks[0].get("num_threads", [0]):
            file = os.path.join(data, f"{prog}-{thread}/package-0.csv")
            df = pd.read_csv(file)

            plt.plot(df["relative_time"] / 1e9, df["energy (microjoule)"] / 1e6, label=f"# {thread} thr.")

        plt.title("Core energy")
        plt.xlabel("Execution time (s)")
        plt.ylabel("Energy (Joule)")
        plt.legend()
        plt.grid()
        plt.show()

    _()
    return


@app.cell
def _(mo):
    mo.md(r"""# Energy comparison""")
    return


@app.cell
def _(data, os, pd, plt, tasks):
    def _ ():
        prog = os.path.basename(tasks[0]["executable"])

        for thread in tasks[0].get("num_threads", [1]):
            file = os.path.join(data, f"{prog}-{thread}/package-0-uncore.csv")
            df = pd.read_csv(file)

            plt.plot(df["relative_time"] / 1e9, df["energy (microjoule)"] / 1e6, label=f"# {thread} thr.")

        for thread in tasks[0].get("num_threads", [0]):
            for size in tasks[0].get("enclave_size", ["64M"]):
                for storage in tasks[0].get("storage_type", ["untrusted"]):
                    file = os.path.join(data, f"sgx-{prog}-{thread}-{size}-{storage}/package-0-uncore.csv")
            df = pd.read_csv(file)

            plt.plot(df["relative_time"] / 1e9, df["energy (microjoule)"] / 1e6, label=f"# sgx-{thread} thr.  {size} {storage}")

        plt.title("Core energy")
        plt.xlabel("Execution time (s)")
        plt.ylabel("Energy (Joule)")
        plt.legend(loc="upper right")
        plt.grid()
        plt.show()

    _()
    return


@app.cell
def _(data, os, pd, plt, tasks):
    def _ ():
        prog = os.path.basename(tasks[1]["executable"])

        for thread in tasks[1].get("num_threads", [1]):
            file = os.path.join(data, f"{prog}-{thread}/package-0-uncore.csv")
            df = pd.read_csv(file)

            plt.plot(df["relative_time"] / 1e9, df["energy (microjoule)"] / 1e6, label=f"# {thread} thr.")

        for thread in tasks[1].get("num_threads", [1]):
            for size in tasks[1].get("enclave_size", ["64M"]):
                for storage in tasks[1].get("storage_type", ["untrusted"]):
                    file = os.path.join(data, f"sgx-{prog}-{thread}-{size}-{storage}/package-0-uncore.csv")
            df = pd.read_csv(file)

            plt.plot(df["relative_time"] / 1e9, df["energy (microjoule)"] / 1e6, label=f"# sgx-{thread} thr.  {size} {storage}")

        plt.title("Core energy")
        plt.xlabel("Execution time (s)")
        plt.ylabel("Energy (Joule)")
        plt.legend(loc="upper right")
        plt.grid()
        plt.show()

    _()
    return


@app.cell
def _():
    return


if __name__ == "__main__":
    app.run()
