use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;

fn main() {
    let out_dir = PathBuf::from(&env::var("OUT_DIR").unwrap());
    let target = env::var("TARGET").unwrap();

    let library_dir = out_dir.join("sundials");

    if !library_dir.exists() {
        fetch_and_build_sundials(&library_dir, &target);
    }

    println!(
        "cargo:rustc-link-search=native={}",
        library_dir.join("lib").display()
    );

    let lib_suffix = if target.contains("windows") {
        "_static"
    } else {
        ""
    };

    for lib in [
        "core",
        "cvode",
        "nvecserial",
        "sunlinsoldense",
        "sunmatrixdense",
    ] {
        println!("cargo:rustc-link-lib=static=sundials_{lib}{lib_suffix}");
    }
}

/// Downloads, builds, and installs SUNDIALS to the specified directory.
fn fetch_and_build_sundials(install_dir: &Path, target: &str) {
    let version = "7.8.0";
    let out_dir = env::var("OUT_DIR").expect("OUT_DIR not set");
    let out_path = Path::new(&out_dir);

    println!(
        "cargo:warning=Sundials not found. Downloading and building SUNDIALS v{version} for {target}..."
    );

    // Determine CMake generator based on the Rust target
    let generator = if target.contains("msvc") {
        "Visual Studio 17 2022"
    } else if target.contains("windows-gnu") {
        "MinGW Makefiles"
    } else {
        "Unix Makefiles"
    };

    // 1. Download source using curl (standard on modern Windows)
    let url =
        format!("https://github.com/llnl/sundials/releases/download/v{version}/sundials-{version}.tar.gz");
    let tar_path = out_path.join("sundials.tar.gz");
    let status = Command::new("curl")
        .args(["-L", "-o", tar_path.to_str().unwrap(), &url])
        .output()
        .expect("Failed to execute curl. Ensure it is installed and in your PATH.");

    if !status.status.success() {
        panic!(
            "Failed to download SUNDIALS from {}. Error: {}",
            url,
            String::from_utf8_lossy(&status.stderr)
        );
    }

    // 2. Extract source using cmake -E tar
    let status = Command::new("cmake")
        .args(["-E", "tar", "xzf", tar_path.to_str().unwrap()])
        .current_dir(out_path)
        .output()
        .expect("Failed to execute cmake -E tar.");
    if !status.status.success() {
        panic!(
            "Failed to extract SUNDIALS source: {}",
            String::from_utf8_lossy(&status.stderr)
        );
    }

    let src_dir = out_path.join(format!("sundials-{version}"));
    let build_dir = out_path.join("sundials-build");

    // 3. Configure with CMake
    let status = Command::new("cmake")
        .args([
            "-S",
            src_dir.to_str().unwrap(),
            "-B",
            build_dir.to_str().unwrap(),
            &format!("-DCMAKE_INSTALL_PREFIX={}", install_dir.display()),
            "-DBUILD_SHARED_LIBS=OFF",
            "-DBUILD_TESTING=OFF",
            "-DSUNDIALS_ENABLE_ARKODE=OFF",
            "-DSUNDIALS_ENABLE_CVODES=OFF",
            "-DSUNDIALS_ENABLE_C_EXAMPLES=OFF",
            "-DSUNDIALS_ENABLE_IDA=OFF",
            "-DSUNDIALS_ENABLE_IDAS=OFF",
            "-DSUNDIALS_ENABLE_KINSOL=OFF",
            "-G",
            generator,
        ])
        .output()
        .expect("Failed to execute cmake. Ensure it is installed and in your PATH.");

    if !status.status.success() {
        panic!(
            "Failed to configure SUNDIALS with generator {}: {}",
            generator,
            String::from_utf8_lossy(&status.stderr)
        );
    }

    // 4. Build and Install
    let status = Command::new("cmake")
        .args([
            "--build",
            build_dir.to_str().unwrap(),
            "--config",
            "Release",
            "--target",
            "install",
        ])
        .output()
        .expect("Failed to build SUNDIALS.");

    if !status.status.success() {
        panic!(
            "Failed to install SUNDIALS: {}",
            String::from_utf8_lossy(&status.stderr)
        );
    }
}
