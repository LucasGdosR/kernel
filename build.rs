// Copyright(c) The Maintainers of Nanvix.
// Licensed under the MIT License.

//==================================================================================================
// Configuration
//==================================================================================================

#![deny(clippy::all)]

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
    let out_dir = setup_path();
    let cflags = setup_toolchain();    
    let asm_sources = collect_sources();
    
    let object_files = compile_source(asm_sources, cflags, &out_dir);

    build_archive(object_files, &out_dir);
    link_archive(out_dir);
}

//==============================================================================================
// Get Essential Environment Variables
//==============================================================================================    

fn setup_path() -> String {
    // Get OUT_DIR environment variable.
    match env::var("OUT_DIR") {
        Ok(out_dir) => out_dir,
        Err(_) => panic!("failed to get OUT_DIR environment variable"),
    }
}

//==============================================================================================
// Configure Toolchain
//==============================================================================================

fn setup_toolchain<'a>() -> Vec<&'a str> {
    let mut cflags: Vec<&'a str> = vec![
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

    cflags
}

//==============================================================================================
// Collect Assembly Source Files
//==============================================================================================

fn collect_sources() -> Vec<String> {
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
    asm_sources
}

//==============================================================================================
// Compile Assembly Source Files
//==============================================================================================

fn compile_source(asm_sources: Vec<String>, cflags: Vec<&str>, out_dir: &String) -> Vec<String> {
    // Compile assembly source files and collect object files.
    let mut object_files: Vec<String> = Vec::<String>::new();
    for asm in asm_sources.iter() {
        let obj: String =
            format!("{}/{}.o", out_dir, Path::new(asm).file_stem().unwrap().to_str().unwrap());

        let status: ExitStatus = Command::new("gcc".to_string())
            .args(&cflags)
            .args(["-c", asm, "-o", &obj])
            .status()
            .unwrap();

        if !status.success() {
            panic!("failed to compile {}", asm);
        }

        println!("cargo::rerun-if-changed={}", asm);
        object_files.push(obj);
    }
    object_files
}

//==============================================================================================
// Build Archive with Object Files
//==============================================================================================

fn build_archive(object_files: Vec<String>, out_dir: &String) {
    let status: ExitStatus = Command::new("ar")
        .args(["rcs", "libkernel.a"])
        .args(object_files)
        .current_dir(Path::new(out_dir))
        .status()
        .unwrap();
    if !status.success() {
        panic!("failed to archive object files");
    }
}

//==============================================================================================
// Link Archive
//==============================================================================================

fn link_archive(out_dir: String) {
    println!("cargo::rustc-link-search=native={}", out_dir);
}
