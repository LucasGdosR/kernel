// Copyright(c) The Maintainers of Nanvix.
// Licensed under the MIT License.

//==================================================================================================
// Imports
//==================================================================================================

use ::std::{
    env,
    path::Path,
    process::{
        Command,
        ExitStatus,
    },
};

//==================================================================================================
// Main Function
//==================================================================================================

fn main() {
    //==============================================================================================
    // Get Essential Environment Variables
    //==============================================================================================

    // Get OUT_DIR environment variable.
    let out_dir: String = match env::var("OUT_DIR") {
        Ok(out_dir) => out_dir,
        Err(_) => panic!("failed to get OUT_DIR environment variable"),
    };

    // Get PATH environment variable.
    let path: String = match env::var("PATH") {
        Ok(path) => path,
        Err(_) => panic!("failed to get PATH environment variable"),
    };

    // Get TOOLCHAIN_DIR environment variable.
    let toolchain_dir: String = match env::var("TOOLCHAIN_DIR") {
        Ok(toolchain_dir) => toolchain_dir,
        Err(_) => panic!("failed to get TOOLCHAIN_DIR environment variable"),
    };

    // Add cross-compiler toolchain to PATH as `targets/*.json` relies on this.
    let gcc_home: String = format!("{}/i486/bin", toolchain_dir);
    let path: String = format!("{}:{}", gcc_home, path);
    println!("cargo::rustc-env=PATH={}", path);
    println!("cargo::rerun-if-env-changed=TOOLCHAIN_DIR");

    //==============================================================================================
    // Configure Toolchain
    //==============================================================================================

    let cc: String = format!("{}/i486-elf-gcc", gcc_home);

    let mut cflags: Vec<&str> = vec![
        "-nostdlib",
        "-ffreestanding",
        "-march=pentiumpro",
        "-Wa,-march=pentiumpro",
        "-Wstack-usage=4096",
        "-Wall",
        "-m32",
        "-Wextra",
        "-Werror",
    ];

    cfg_if::cfg_if! {
        if #[cfg(debug_assertions)] {
            cflags.push("-O0");
            cflags.push("-g");
        } else {
            cflags.push("-O3");
        }
    }

    // Check for microvm feature
    cfg_if::cfg_if! {
        if #[cfg(feature = "microvm")] {
            cflags.push("-D__microvm__");
        }
        else {
            cflags.push("-D__pc__");
        }
    }

    //==============================================================================================
    // Collect Assembly Source Files
    //==============================================================================================

    let sources_dir: Vec<&str> = vec!["src/hal/arch/x86"];

    // Collect *.S files in the sources directory
    let mut asm_sources = Vec::<String>::new();
    for dir in sources_dir.iter() {
        for entry in Path::new(dir).read_dir().unwrap() {
            let entry = entry.unwrap();
            let path = entry.path();
            if let Some(extension) = path.extension() {
                if extension == "S" {
                    let path = path.to_str().unwrap().to_string();
                    asm_sources.push(path);
                }
            }
        }
    }

    //==============================================================================================
    // Compile Assembly Source Files
    //==============================================================================================

    // Compile assembly source files and collect object files.
    let mut object_files: Vec<String> = Vec::<String>::new();
    for asm in asm_sources.iter() {
        let obj: String =
            format!("{}/{}.o", out_dir, Path::new(asm).file_stem().unwrap().to_str().unwrap());

        let status: ExitStatus = Command::new(cc.clone())
            .args(&cflags)
            .args(&["-c", asm, "-o", &obj])
            .status()
            .unwrap();

        if !status.success() {
            panic!("failed to compile {}", asm);
        }

        println!("cargo::rerun-if-changed={}", asm);
        object_files.push(obj);
    }

    //==============================================================================================
    // Build Archive with Object Files
    //==============================================================================================

    let status: ExitStatus = Command::new("ar")
        .args(&["rcs", "libkernel.a"])
        .args(&object_files)
        .current_dir(&Path::new(&out_dir))
        .status()
        .unwrap();
    if !status.success() {
        panic!("failed to archive object files");
    }

    //==============================================================================================
    // Link Archive
    //==============================================================================================

    println!("cargo::rustc-link-search=native={}", out_dir);
    println!("cargo::rustc-link-lib=static:+whole-archive=kernel");
}
