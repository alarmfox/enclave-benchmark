import marimo

__generated_with = "0.11.6"
app = marimo.App(width="medium")


@app.cell
def _(mo):
    mo.md(r"""## Energy consumption""")
    return


@app.cell
def _():
    return


@app.cell
def _():
    import marimo as mo
    return (mo,)


if __name__ == "__main__":
    app.run()
