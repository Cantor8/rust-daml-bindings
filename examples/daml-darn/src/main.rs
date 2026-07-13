#![doc = include_str!("../README.md")]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, rust_2018_idioms)]
#![allow(
    clippy::module_name_repetitions,
    clippy::use_self,
    clippy::must_use_candidate,
    clippy::missing_errors_doc,
    clippy::cast_sign_loss
)]
#![forbid(unsafe_code)]

use anyhow::Result;
use clap::{Parser, Subcommand};

use crate::command_intern::{intern_dotted, intern_string, SortOrder};
use crate::command_package::show_package;

mod command_intern;
mod command_package;

/// Tools for working with Daml Archives.
#[derive(Parser)]
#[command(version, about)]
struct Cli {
    #[command(subcommand)]
    command: DarnCommand,
}

#[derive(Subcommand)]
enum DarnCommand {
    /// Show DAR package details.
    Package {
        /// Path to the DAR file.
        dar: String,
    },
    /// Show interned strings and dotted names in a DAR.
    Intern {
        /// Path to the DAR file.
        dar: String,
        /// Show interned strings.
        #[arg(short, long, conflicts_with = "dotted")]
        string: bool,
        /// Show interned dotted names.
        #[arg(short, long)]
        dotted: bool,
        /// Restrict output to these intern indices (comma-separated).
        #[arg(short, long, value_delimiter = ',')]
        index: Vec<usize>,
        /// Include names that start with `$` (compiler-mangled).
        #[arg(short = 'f', long)]
        show_mangled: bool,
        /// Sort output by intern index.
        #[arg(long, conflicts_with = "order_by_name")]
        order_by_index: bool,
        /// Sort output by rendered name (default).
        #[arg(long)]
        order_by_name: bool,
    },
}

fn main() -> Result<()> {
    match Cli::parse().command {
        DarnCommand::Package {
            dar,
        } => show_package(&dar),
        DarnCommand::Intern {
            dar,
            string,
            dotted,
            index,
            show_mangled,
            order_by_index,
            order_by_name: _,
        } => {
            if !string && !dotted {
                anyhow::bail!("one of --string or --dotted is required");
            }
            let sort = if order_by_index { SortOrder::ByIndex } else { SortOrder::ByName };
            if dotted {
                intern_dotted(&dar, show_mangled, &sort, &index)
            } else {
                intern_string(&dar, show_mangled, &sort, &index)
            }
        },
    }
}
