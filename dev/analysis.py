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

    config = "examples/demo.toml"
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
        prog = os.path.basename(tasks[1]["executable"])
    
        for thread in tasks[0].get("num_threads", [1]):
            file = os.path.join(data, f"{prog}-{thread}-untrusted/package-0.csv")
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
def _(data, os, pd, plt, prog, threads):
    def _():
        for thread in threads:
            file = os.path.join(data, f"{prog}-{thread}-untrusted/deep-trace/package-0.csv")
            df = pd.read_csv(file)

            df["relative_time"] = df["timestamp (us)"] - df["timestamp (us)"].loc[0]
            plt.plot(df["timestamp (us)"] / 1e9, df["energy (microjoule)"] / 1e6, label=f"# {thread} thr.")

        plt.title("Core energy")
        plt.xlabel("Execution time (s)")
        plt.ylabel("Energy (Joule)")
        plt.legend()
        plt.grid()
        return plt.show()


    _()
    return


@app.cell
def _():
    return


if __name__ == "__main__":
    app.run()
