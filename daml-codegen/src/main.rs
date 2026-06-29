use daml_codegen::generator::{daml_codegen_internal, ModuleOutputMode, RenderMethod};

use clap::{crate_description, crate_name, crate_version, Arg, ArgAction, Command};

fn main() {
    let matches = Command::new(crate_name!())
        .version(crate_version!())
        .about(crate_description!())
        .arg(Arg::new("dar").help("Sets the input Dar file to use").required(true).index(1))
        .arg(Arg::new("output").short('o').long("output-dir").num_args(1).help("Sets the output path"))
        .arg(
            Arg::new("verbose")
                .short('v')
                .long("verbose")
                .action(ArgAction::Count)
                .help("Sets the level of verbosity"),
        )
        .arg(
            Arg::new("filter")
                .short('f')
                .long("module-filter")
                .num_args(1..)
                .help("Sets the regex module filter to apply"),
        )
        .arg(
            Arg::new("intermediate")
                .short('i')
                .long("render-intermediate")
                .action(ArgAction::SetTrue)
                .help("Generate intermediate types"),
        )
        .arg(
            Arg::new("combine")
                .short('c')
                .long("combine-modules")
                .action(ArgAction::SetTrue)
                .help("Combine modules as a single file"),
        )
        .get_matches();
    let dar_file = matches.get_one::<String>("dar").map(String::as_str).unwrap();
    let output_path = matches.get_one::<String>("output").map(String::as_str).unwrap_or(".");
    let filters: Vec<&str> =
        matches.get_many::<String>("filter").map(|v| v.map(String::as_str).collect()).unwrap_or_default();
    let render_method =
        if matches.get_flag("intermediate") { RenderMethod::Intermediate } else { RenderMethod::Full };
    let module_output_mode =
        if matches.get_flag("combine") { ModuleOutputMode::Combined } else { ModuleOutputMode::Separate };
    daml_codegen_internal(dar_file, output_path, &filters, render_method, module_output_mode)
        .expect("failed to generate code for Daml archive");
}
