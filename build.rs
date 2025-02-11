use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::{env, fs};

use libbpf_cargo::SkeletonBuilder;

const SRC: &str = "src/bpf/tracer.bpf.c";
const HEADER: &str = "src/bpf/tracer.def.h";

fn main() {
    let skip_sgx = env::var_os("EB_SKIP_SGX").is_some_and(|s| s == "1");
    let manifest_dir = PathBuf::from(
        env::var_os("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR must be set in build script"),
    );

    // inject #define EB_SKIP_SGX in tracer.h if skip_sgx is true
    let header = fs::read_to_string(HEADER).expect("cannot find header file");
    let mut lines = header.lines().collect::<Vec<&str>>();
    if skip_sgx && !header.contains("#define EB_SKIP_SGX") {
        // skip #ifndef ...
        lines.insert(2, "#define EB_SKIP_SGX");
    }
    lines.insert(
        0,
        "/* DO NOT EDIT THIS FILE. THIS IS AUTOGENERATED FROM build.rs. EDIT tracer.def.h */",
    );
    fs::write(
        manifest_dir.join("src").join("bpf").join("tracer.h"),
        lines.join("\n").as_bytes(),
    )
    .expect("cannot write header file");

    // bpftool btf dump file /sys/kernel/btf/vmlinux format c > src/bpf/vmlinux.h
    let vmlinux = Command::new("bpftool")
        .args([
            "btf",
            "dump",
            "file",
            "/sys/kernel/btf/vmlinux",
            "format",
            "c",
        ])
        .stdout(Stdio::piped())
        .output()
        .expect("cannot generate vmlinux.h");

    fs::write(manifest_dir.join("src/bpf/vmlinux.h"), vmlinux.stdout)
        .expect("cannot write src/bpf/vmlinux.h");
    let out = manifest_dir.join("src").join("bpf").join("tracer.skel.rs");

    SkeletonBuilder::new()
        .source(SRC)
        .build_and_generate(&out)
        .unwrap();
    println!("cargo:rerun-if-changed={SRC}");
    println!("cargo:rerun-if-changed={HEADER}");
}
