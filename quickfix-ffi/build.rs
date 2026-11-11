//! Builds libquickfix and the C binding as static libraries with `cmake`, and links them.

use std::{env, fmt, fs, io, path::Path};

use cmake::Config;
use fs_extra::dir::CopyOptions;

/// Why the build script stopped. `main` returning it prints the `Debug` form, so that is the
/// message.
enum BuildError {
    /// The build never fetches anything itself: a missing submodule is the caller's to fix (R-P1-5).
    MissingSubmodule,
    MissingOutDir,
    Copy(String),
    RemoveOldCopy(io::Error),
}

impl fmt::Debug for BuildError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingSubmodule => f.write_str(
                "the libquickfix submodule is missing: run `git submodule update --init \
                 --recursive` in the quickfix-rs checkout (the build does not fetch it)",
            ),
            Self::MissingOutDir => f.write_str("OUT_DIR is not set (run through cargo)"),
            Self::Copy(err) => write!(f, "cannot copy libquickfix to OUT_DIR: {err}"),
            Self::RemoveOldCopy(err) => write!(f, "cannot remove the old libquickfix copy: {err}"),
        }
    }
}

fn have_feature(flag: &str) -> bool {
    env::var(format!(
        "CARGO_FEATURE_{}",
        flag.replace('-', "_").to_uppercase()
    ))
    .is_ok()
}

fn read_cmake_opt(flag: &str) -> &'static str {
    if have_feature(flag) {
        "ON"
    } else {
        "OFF"
    }
}

/// Compiler cache, only when asked for: `QUICKFIX_FFI_COMPILER_LAUNCHER=sccache` (R-P1-5).
fn get_compiler_launcher() -> Option<String> {
    env::var("QUICKFIX_FFI_COMPILER_LAUNCHER")
        .ok()
        .filter(|launcher| !launcher.is_empty())
}

/// `CMAKE_<LANG>_FLAGS` for both C++ builds, or `None` for MSVC (cmake-rs defaults).
///
/// Set here because cmake-rs adds `-w` to its own flags, which hides every compiler warning.
/// Adds the hardening flags of R-P1-5 and keeps the user's `CFLAGS`/`CXXFLAGS` (sanitizers).
fn cmake_flags(target_os: TargetOs, env_var: &str, include_dir: Option<&str>) -> Option<String> {
    if target_os == TargetOs::Windows {
        return None;
    }
    let user_flags = env::var(env_var).unwrap_or_default();
    let manifest_dir = env::var("CARGO_MANIFEST_DIR").unwrap_or_default();
    let out_dir = env::var("OUT_DIR").unwrap_or_default();
    let mut flags: Vec<String> = include_dir
        .map(|dir| format!("-I{dir}"))
        .into_iter()
        .collect();
    flags.extend([
        "-fPIC".to_owned(),
        "-ffunction-sections".to_owned(),
        "-fdata-sections".to_owned(),
        "-Wall".to_owned(),
        "-fno-omit-frame-pointer".to_owned(),
        "-fstack-protector-strong".to_owned(),
        // Same objects whatever the checkout or build directory.
        format!("-ffile-prefix-map={out_dir}=/quickfix-ffi-out"),
        format!("-ffile-prefix-map={manifest_dir}=/quickfix-ffi"),
    ]);
    if target_os == TargetOs::Linux {
        flags.push("-fstack-clash-protection".to_owned());
        // FORTIFY's checked copies bypass the sanitizers' interceptors.
        if !user_flags.contains("-fsanitize") {
            flags.push("-U_FORTIFY_SOURCE".to_owned());
            flags.push("-D_FORTIFY_SOURCE=2".to_owned());
        }
    }
    flags.push(user_flags);
    Some(flags.join(" "))
}

fn set_cmake_flags(config: &mut Config, target_os: TargetOs, include_dir: Option<&str>) {
    for (lang, env_var) in [("C", "CFLAGS"), ("CXX", "CXXFLAGS")] {
        println!("cargo:rerun-if-env-changed={env_var}");
        if let Some(flags) = cmake_flags(target_os, env_var, include_dir) {
            config.define(format!("CMAKE_{lang}_FLAGS"), flags);
        }
    }
}

fn main() -> Result<(), BuildError> {
    let out_dir = env::var("OUT_DIR").map_err(|_| BuildError::MissingOutDir)?;
    let target_os = TargetOs::from_env();

    if !Path::new("libquickfix/LICENSE").exists() {
        return Err(BuildError::MissingSubmodule);
    }

    // Tell Cargo that if the given file changes, to rerun this build script.
    println!("cargo:rerun-if-changed=./CMakeLists.txt");
    println!("cargo:rerun-if-changed=./libquickfix");
    println!("cargo:rerun-if-changed=./quickfix-bind");
    println!("cargo:rerun-if-env-changed=QUICKFIX_FFI_COMPILER_LAUNCHER");

    // Clone libquickfix to OUT_DIR because it modify itself when building
    let libquickfix_build_dir = Path::new(&out_dir).join("libquickfix");

    match fs::remove_dir_all(&libquickfix_build_dir) {
        Err(err) if err.kind() != io::ErrorKind::NotFound => {
            return Err(BuildError::RemoveOldCopy(err))
        }
        _ => {}
    }
    fs_extra::copy_items(&["./libquickfix"], &out_dir, &CopyOptions::default())
        .map_err(|err| BuildError::Copy(err.to_string()))?;

    // Build quickfix as a static library
    let mut quickfix_cmake_config = Config::new(libquickfix_build_dir);
    // Its own build directory, so each build's CMake cache shows the flags it was built with.
    quickfix_cmake_config.out_dir(Path::new(&out_dir).join("libquickfix-out"));
    set_cmake_flags(&mut quickfix_cmake_config, target_os, None);
    quickfix_cmake_config
        .define("CMAKE_POLICY_VERSION_MINIMUM", "3.10")
        .define("HAVE_SSL", read_cmake_opt("build-with-ssl"))
        .define("HAVE_MYSQL", read_cmake_opt("build-with-mysql"))
        .define("HAVE_POSTGRESQL", read_cmake_opt("build-with-postgres"))
        .define("HAVE_PYTHON", "OFF")
        .define("HAVE_PYTHON3", "OFF")
        .define("QUICKFIX_SHARED_LIBS", "OFF")
        .define("QUICKFIX_EXAMPLES", "OFF")
        .define("QUICKFIX_TESTS", "OFF")
        // Always compile libquickfix in release mode.
        // We are not here to debug this library.
        .profile("RelWithDebInfo");

    if let Some(compiler_launcher) = get_compiler_launcher() {
        quickfix_cmake_config
            .define("CMAKE_C_COMPILER_LAUNCHER", &compiler_launcher)
            .define("CMAKE_CXX_COMPILER_LAUNCHER", &compiler_launcher);
    }

    let quickfix_dst = quickfix_cmake_config.build();
    let quickfix_include_path = format!("{}/include", quickfix_dst.display());
    let quickfix_lib_path = format!("{}/lib", quickfix_dst.display());

    // Build quickfix C bind also as a static library.
    env::set_var("CMAKE_LIBRARY_PATH", [quickfix_lib_path].join(";"));

    let mut quickfix_bind_cmake_config = Config::new(".");
    // MSVC keeps cmake-rs's flags, so it needs the include path there too.
    set_cmake_flags(
        &mut quickfix_bind_cmake_config,
        target_os,
        Some(&quickfix_include_path),
    );
    quickfix_bind_cmake_config
        .cflag(format!("-I{quickfix_include_path}"))
        .cxxflag(format!("-I{quickfix_include_path}"))
        .define("QUICKFIX_BIND_EXAMPLES", "OFF")
        .define("HAVE_SSL", read_cmake_opt("build-with-ssl"))
        .define("HAVE_MYSQL", read_cmake_opt("build-with-mysql"))
        .define("HAVE_POSTGRESQL", read_cmake_opt("build-with-postgres"));

    if let Some(compiler_launcher) = get_compiler_launcher() {
        quickfix_bind_cmake_config
            .define("CMAKE_C_COMPILER_LAUNCHER", &compiler_launcher)
            .define("CMAKE_CXX_COMPILER_LAUNCHER", &compiler_launcher);
    }

    let quickfix_bind_dst = quickfix_bind_cmake_config.build();

    print_link_flags(target_os, &quickfix_dst, &quickfix_bind_dst);
    Ok(())
}

/// Tells rustc where the two static libraries are and what else to link.
fn print_link_flags(target_os: TargetOs, quickfix_dst: &Path, quickfix_bind_dst: &Path) {
    println!(
        "cargo:rustc-link-search=native={}/lib",
        quickfix_dst.display()
    );
    println!(
        "cargo:rustc-link-search=native={}/lib",
        quickfix_bind_dst.display()
    );

    // ⚠️ NOTE: libquickfix as a different name on windows with debug profile.
    println!("cargo:rustc-link-lib=static=quickfix");
    println!("cargo:rustc-link-lib=static=quickfixbind");

    // Lib std C++ is only available on UNIX platform.
    if let Some(lib_cpp_name) = target_os.lib_std_cpp_name() {
        println!("cargo:rustc-link-lib={lib_cpp_name}");
    }

    // Link with external libraries if needed.
    if have_feature("build-with-ssl") {
        println!("cargo:rustc-link-lib=ssl");
        println!("cargo:rustc-link-lib=crypto");
    }
    if have_feature("build-with-mysql") {
        println!("cargo:rustc-link-lib=mysqlclient");
    }
    if have_feature("build-with-postgres") {
        println!("cargo:rustc-link-lib=pq");
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
enum TargetOs {
    Windows,
    Linux,
    Other,
}

impl TargetOs {
    fn from_env() -> Self {
        match env::var("CARGO_CFG_TARGET_OS").as_deref() {
            Ok("windows") => Self::Windows,
            Ok("linux") => Self::Linux,
            _ => Self::Other,
        }
    }

    fn lib_std_cpp_name(self) -> Option<&'static str> {
        match self {
            Self::Windows => None,
            Self::Linux => Some("stdc++"),
            Self::Other => Some("c++"),
        }
    }
}
