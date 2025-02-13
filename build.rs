use std::env;
use std::path::PathBuf;

use libbpf_cargo::SkeletonBuilder;

const SRC: &str = "src/bpf/tracer.bpf.c";

fn main() {
  let manifest_dir = PathBuf::from(
    env::var_os("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR must be set in build script"),
  );

  let out = manifest_dir.join("src").join("bpf").join("tracer.skel.rs");

  SkeletonBuilder::new()
    .source(SRC)
    .build_and_generate(&out)
    .unwrap();
  println!("cargo:rerun-if-changed={SRC}");
}
