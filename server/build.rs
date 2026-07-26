use std::{env, path::PathBuf, process::Command};

fn main() {
    if env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos") {
        cc::Build::new()
            .file("src/collect/udp/macos.c")
            .warnings(true)
            .compile("netcart_udp_macos");
        println!("cargo:rustc-link-lib=framework=CoreFoundation");
        println!("cargo:rerun-if-changed=src/collect/udp/macos.c");
    }

    println!("cargo:rerun-if-changed=../ui");
    println!("cargo:rerun-if-changed=../package.json");
    println!("cargo:rerun-if-changed=../vite.config.ts");

    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let project_dir = manifest_dir
        .parent()
        .expect("backend lives below project root");
    if project_dir.join("dist/index.html").is_file() {
        return;
    }

    let npm = if cfg!(windows) { "npm.cmd" } else { "npm" };
    let status = Command::new(npm)
        .args(["run", "build"])
        .current_dir(project_dir)
        .status()
        .expect("failed to start `npm run build`; install Node.js and run `npm install`");
    assert!(status.success(), "`npm run build` failed");
}
