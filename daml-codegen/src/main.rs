use std::error::Error;
use std::process::ExitCode;

use daml_codegen::generator::{ModuleOutputMode, RenderMethod, daml_codegen_internal};

use clap::{Arg, ArgAction, Command, crate_description, crate_name, crate_version};

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("daml-codegen: {e:#}");
            ExitCode::FAILURE
        },
    }
}

fn run() -> Result<(), Box<dyn Error>> {
    let matches = Command::new(crate_name!())
        .version(crate_version!())
        .about(crate_description!())
        .arg(Arg::new("dar").help("Path to the input DAR file").required(true).index(1))
        .arg(Arg::new("output").short('o').long("output-dir").num_args(1).help("Output directory (default: '.')"))
        .arg(Arg::new("filter").short('f').long("module-filter").num_args(1..).help(
            "Regex(es) matched against fully-qualified module names \
                     (e.g. 'MyPackage\\.Foo\\..*'). Only modules matching any \
                     of the regexes are emitted; without --module-filter, \
                     every module is emitted.",
        ))
        .arg(
            Arg::new("intermediate")
                .short('i')
                .long("render-intermediate")
                .action(ArgAction::SetTrue)
                .help("Render an intermediate representation instead of the full Rust types (debugging aid)"),
        )
        .arg(
            Arg::new("combine")
                .short('c')
                .long("combine-modules")
                .action(ArgAction::SetTrue)
                .help("Emit one Rust file per DAR (default: one file per module)"),
        )
        .get_matches();
    let dar_file = matches.get_one::<String>("dar").map(String::as_str).unwrap();
    let output_path = matches.get_one::<String>("output").map(String::as_str).unwrap_or(".");
    let filters: Vec<&str> =
        matches.get_many::<String>("filter").map(|v| v.map(String::as_str).collect()).unwrap_or_default();
    let render_method = if matches.get_flag("intermediate") {
        RenderMethod::Intermediate
    } else {
        RenderMethod::Full
    };
    let module_output_mode = if matches.get_flag("combine") {
        ModuleOutputMode::Combined
    } else {
        ModuleOutputMode::Separate
    };
    daml_codegen_internal(dar_file, output_path, &filters, render_method, module_output_mode)?;
    Ok(())
}
