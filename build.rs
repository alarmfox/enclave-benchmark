use std::path::PathBuf;
use std::{env, fs};

use libbpf_cargo::SkeletonBuilder;

const SRC: &str = "src/bpf/tracer.bpf.c";
const HEADER: &str = "src/bpf/tracer.def.h";

fn main() {
    let skip_sgx = env::var("EB_SKIP_SGX").is_ok_and(|s| s == "1");
    let manifest_dir = PathBuf::from(
        env::var_os("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR must be set in build script"),
    );

    // inject #define EB_SKIP_SGX = 1 in tracer.h if skip_sgx is true
    if skip_sgx {
        let header = fs::read_to_string(HEADER).expect("cannot find header file");
        let mut lines = header.lines().collect::<Vec<&str>>();
        if !header.contains("#define EB_SKIP_SGX") {
            // skip #ifndef ...
            lines.insert(2, "#define EB_SKIP_SGX");
        }
        fs::write(
            manifest_dir.join("src").join("bpf").join("tracer.h"),
            lines.join("\n").as_bytes(),
        )
        .expect("cannot write header file");
    } else {
        fs::copy(
            HEADER,
            manifest_dir.join("src").join("bpf").join("tracer.h"),
        )
        .expect("cannot copy header file");
    }

    let out = manifest_dir.join("src").join("bpf").join("tracer.skel.rs");

    SkeletonBuilder::new()
        .source(SRC)
        .build_and_generate(&out)
        .unwrap();
    println!("cargo:rerun-if-changed={SRC}");
    println!("cargo:rerun-if-changed={HEADER}");
}
