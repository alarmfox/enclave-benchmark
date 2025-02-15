import json
import sys
import os
import subprocess

if len(sys.argv) != 3:
    print("Usage python generate_docs.py <path/to/rust/project> <path/to/sphinx>")
    sys.exit(1)

# Adjust PROJECT_ROOT as needed. This example assumes the project root is two levels up.
PROJECT_ROOT, DOCS_SOURCE = os.path.abspath(sys.argv[1]), os.path.abspath(sys.argv[2])
GENERATED_JSON_FILE = os.path.join(PROJECT_ROOT, "target/doc/enclave_benchmark.json")

print("Project root:", PROJECT_ROOT)
print("Documentation source:", DOCS_SOURCE)

def extract_source(span):
    """Extracts source code from a Rust file using span info."""
    filename = span.get("filename")
    begin = span.get("begin", [0])[0] - 1  # Convert to 0-based index
    end = span.get("end", [0])[0]          # 1-based index

    if not filename:
        return None

    full_path = os.path.join(PROJECT_ROOT, filename)
    if not os.path.exists(full_path):
        print(f"Warning: Source file not found: {full_path}")
        return None

    try:
        with open(full_path, "r") as file:
            lines = file.readlines()
            return "".join(lines[begin:end])
    except FileNotFoundError:
        return None

def generate_rst(items):
    # Group public items (with documentation) by source file (span.filename)
    groups = {}
    for key, item in items.items():
        if item.get("visibility") != "public" or item.get("docs") is None:
            continue

        span = item.get("span")
        filename = span.get("filename", "unknown") if span else "unknown"
        groups.setdefault(filename, []).append(item)
    
    rst_output = []
    
    # General heading
    main_heading = "Code Documentation"
    rst_output.append(main_heading)
    rst_output.append("=" * len(main_heading))
    rst_output.append("")
    
    # Process each source file group
    for filename in sorted(groups.keys()):
        # Heading for the source file group
        rst_output.append(filename)
        rst_output.append("-" * len(filename))
        rst_output.append("")
        
        # Process each public item in this file
        for item in groups[filename]:
            name = item.get("name")
            doc = (item.get("docs") or "No documentation available.").strip()
            rst_output.append(f"{name}")
            rst_output.append("~" * len(name))
            rst_output.append("")
            rst_output.append(doc)
            rst_output.append("")
            
            # Insert a collapsible block for the source code
            code = extract_source(item.get("span"))
            if code:
                rst_output.append(".. collapse:: Show Code")
                rst_output.append("")
                rst_output.append("   .. code-block:: rust")
                rst_output.append("")
                for line in code.splitlines():
                    rst_output.append("      " + line.rstrip())
                rst_output.append("")
        rst_output.append("")  # blank line between groups

    return "\n".join(rst_output)

print("Generating json file", end="... ")
subprocess.run([
    "cargo", 
    "+nightly", 
    "rustdoc", 
    "--", 
    "-Zunstable-options", 
    "--output-format", 
    "json"
    ], 
    cwd=PROJECT_ROOT,
   stderr=subprocess.PIPE,
   stdout=subprocess.PIPE
)
print("done")

# Load the rustdoc JSON file
print("Loading json file", end="... ")
with open(GENERATED_JSON_FILE, "r") as f:
    rustdoc = json.load(f)
print("done")

# Get the index of items from rustdoc.json
items = rustdoc.get("index", {})

print("Generating rst file", end="...")
# Generate RST content
rst_content = generate_rst(items)
print("done")

output_file = os.path.join(DOCS_SOURCE, "source", "code_docs.rst")
# Save the output as an RST file
with open(output_file, "w") as f:
    f.write(rst_content)

print("RST documentation generated in", output_file)

print("Compiling documentation", end="... ")
subprocess.run([
    "make", 
    "html", 
    ], 
    cwd=DOCS_SOURCE,
    stderr=subprocess.PIPE,
    stdout=subprocess.PIPE
)
print("done")

