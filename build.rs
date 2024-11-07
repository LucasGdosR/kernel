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
// Structures
//==================================================================================================

///
/// # Description
///
/// A compiler cli command with its flags.
///
#[derive(Debug)]
pub struct CompilerCommand<'a> {
    /// Compiler program. E.g.: gcc.
    compiler_program: &'a str,
    /// Flags for the compiler command.
    cflags: Vec<&'a str>,
}

//==================================================================================================
// Main Function
//==================================================================================================

fn main() {
    let out_dir: String = setup_path();
    let toolchain: CompilerCommand = setup_toolchain();    
    let asm_sources: Vec<String> = collect_sources();
    let object_files: Vec<String> = compile_source(asm_sources, toolchain, &out_dir);
    build_archive(object_files, &out_dir);
    link_archive(out_dir);
}

//==============================================================================================
// Get Essential Environment Variables
//==============================================================================================    

///
/// # Description
///
/// Gets the OUT_DIR environment variable. Panics if the variable isn't present.
///
/// # Returns
///
/// The contents of the OUT_DIR environment variable.
///
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

///
/// # Description
///
/// Builds a cli command with a compiler invocation along with the flags used for compilation.
/// Includes optional flags for debugging and microvm.
///
/// # Returns
///
/// A cli command to invoke a compiler with flags.
///
fn setup_toolchain<'a>() -> CompilerCommand<'a> {
    let compiler_program: &str = "gcc";

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

    CompilerCommand { compiler_program, cflags }
}

//==============================================================================================
// Collect Assembly Source Files
//==============================================================================================

///
/// # Description
///
/// Collects *.S files in the sources directory. Non-recursive.
///
/// # Returns
///
/// A vector with all paths to *.S files in the sources directory.
///
fn collect_sources() -> Vec<String> {
    let sources_dir: Vec<&str> = vec!["src/hal/arch/x86"];

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

///
/// # Description
///
/// Compiles assembly source files and collects object files. Panics if compilation fails.
///
/// # Parameters
///
/// - `asm_sources`: Paths of all *.S source files.
/// - `toolchain`: The compiler to be used and the compilation flags.
/// - `out_dir`: Path to store the *.o files.
///
/// # Returns
///
/// A vector with paths to the resulting object files.
///
fn compile_source(asm_sources: Vec<String>, toolchain: CompilerCommand, out_dir: &String) -> Vec<String> {
    let mut object_files: Vec<String> = Vec::<String>::new();
    for asm in asm_sources.iter() {
        let obj: String =
            format!("{}/{}.o", out_dir, Path::new(asm).file_stem().unwrap().to_str().unwrap());

        let status: ExitStatus = Command::new(toolchain.compiler_program)
            .args(&toolchain.cflags)
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

///
/// # Description
///
/// Builds archive with object files. Panics if it fails.
///
/// # Parameters
///
/// - `object_files`: Paths of all recently compiled *.o files.
/// - `out_dir`: Path to the directory with the *.o files.
///
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

///
/// # Description
///
/// Only prints a message.
///
/// # Parameters
///
/// - `out_dir`: Part of the printed message.
///
fn link_archive(out_dir: String) {
    println!("cargo::rustc-link-search=native={}", out_dir);
}
