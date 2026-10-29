//! The prebuilt ONNX Runtime uses Objective-C `@available` checks (CoreML EP),
//! which need `___isPlatformVersionAtLeast` from clang's compiler-rt. rustc
//! links with `-nodefaultlibs`, so link `libclang_rt.osx.a` explicitly.

use std::process::Command;

fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("macos") {
        return;
    }
    let out = Command::new("xcrun")
        .args(["clang", "-print-resource-dir"])
        .output()
        .expect("xcrun clang -print-resource-dir");
    let resource_dir = String::from_utf8(out.stdout).unwrap();
    println!("cargo:rustc-link-search=native={}/lib/darwin", resource_dir.trim());
    println!("cargo:rustc-link-lib=static=clang_rt.osx");
}
