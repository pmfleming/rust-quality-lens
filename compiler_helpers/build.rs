use std::{env, path::PathBuf, process::Command};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

fn output(rustc: &str, args: &[&str]) -> Result<String> {
    let result = Command::new(rustc).args(args).output()?;
    assert!(
        result.status.success(),
        "compiler probe failed: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    Ok(String::from_utf8(result.stdout)?.trim().to_string())
}

fn main() -> Result<()> {
    let rustc = env::var("RUSTC")?;
    let version = output(&rustc, &["--version"])?;
    assert!(
        version == "rustc 1.95.0-nightly (a423f68a0 2026-02-13)",
        "build this adapter with nightly-2026-02-14 and rustc-dev; found {version}"
    );
    let verbose = output(&rustc, &["-vV"])?;
    let host = verbose
        .lines()
        .find_map(|line| line.strip_prefix("host: "))
        .ok_or("compiler host missing")?;
    println!("cargo:rustc-env=RQLENS_DRIVER_HOST={host}");
    let sysroot = output(&rustc, &["--print", "sysroot"])?;
    let compiler = PathBuf::from(&sysroot).join("bin/rustc");
    println!("cargo:rustc-env=RQLENS_DRIVER_RUSTC={}", compiler.display());
    println!("cargo:rustc-env=RQLENS_DRIVER_SYSROOT={sysroot}");
    println!("cargo:rustc-env=RQLENS_DRIVER_VERSION={version}");
    println!("cargo:rustc-link-search=native={sysroot}/lib");
    if cfg!(target_os = "linux") {
        println!("cargo:rustc-link-arg=-Wl,-rpath,{sysroot}/lib");
    }
    println!("cargo:rerun-if-env-changed=RUSTC");
    Ok(())
}
