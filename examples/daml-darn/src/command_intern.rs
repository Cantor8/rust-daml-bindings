use std::str::FromStr;

use anyhow::{Context, Result};
use clap::{Arg, ArgAction, ArgGroup, ArgMatches, Command};
use itertools::Itertools;
use prettytable::color::Color;
use prettytable::format;
use prettytable::{color, Attr, Cell, Row, Table};

use daml::lf::{DamlLfPackage, DarFile};

use crate::DarnCommand;

/// Darn command for displaying interned strings and dotted names.
pub struct CommandIntern {}

impl DarnCommand for CommandIntern {
    fn name(&self) -> &str {
        "intern"
    }

    fn args(&self) -> Command {
        Command::new("intern")
            .about("Show interned strings and dotted names in a dar")
            .arg(Arg::new("dar").help("Sets the input dar file to use").required(true).index(1))
            .arg(Arg::new("string").short('s').long("string").action(ArgAction::SetTrue).help("Show interned strings"))
            .arg(
                Arg::new("dotted")
                    .short('d')
                    .long("dotted")
                    .action(ArgAction::SetTrue)
                    .help("Show interned dotted names"),
            )
            .arg(
                Arg::new("index")
                    .short('i')
                    .long("index")
                    .num_args(1..)
                    .value_delimiter(',')
                    .required(false)
                    .help("the intern indices"),
            )
            .arg(
                Arg::new("show-mangled")
                    .short('f')
                    .long("show-mangled")
                    .action(ArgAction::SetTrue)
                    .required(false)
                    .help("show mangled names"),
            )
            .arg(
                Arg::new("order-by-index")
                    .required(false)
                    .long("order-by-index")
                    .action(ArgAction::SetTrue)
                    .help("order by index"),
            )
            .arg(
                Arg::new("order-by-name")
                    .required(false)
                    .long("order-by-name")
                    .action(ArgAction::SetTrue)
                    .help("order by name"),
            )
            .group(ArgGroup::new("mode").required(true).arg("string").arg("dotted"))
            .group(ArgGroup::new("order").required(false).arg("order-by-index").arg("order-by-name"))
    }

    fn execute(&self, matches: &ArgMatches) -> Result<()> {
        let dar_path = matches.get_one::<String>("dar").map(String::as_str).unwrap();
        let filter: Vec<usize> = matches
            .get_many::<String>("index")
            .map(|vals| {
                vals.map(|i| usize::from_str(i).context(format!("parsing index from '{}'", i)))
                    .collect::<Result<Vec<_>>>()
            })
            .transpose()?
            .unwrap_or_default();
        let show_mangled = matches.get_flag("show-mangled");
        let sort = match (matches.get_flag("order-by-index"), matches.get_flag("order-by-name")) {
            (true, false) => SortOrder::ByIndex,
            _ => SortOrder::ByName,
        };
        if matches.get_flag("dotted") {
            intern_dotted(dar_path, show_mangled, &sort, filter.as_slice())
        } else if matches.get_flag("string") {
            intern_string(dar_path, show_mangled, &sort, filter.as_slice())
        } else {
            unreachable!()
        }
    }
}

enum SortOrder {
    ByIndex,
    ByName,
}

fn intern_string(dar_path: &str, show_mangled: bool, sort_order: &SortOrder, filter: &[usize]) -> Result<()> {
    let dar = DarFile::from_file(dar_path)?;
    match dar.main.payload.package {
        DamlLfPackage::V2(package) => {
            let mut res: Vec<_> = package
                .interned_strings
                .iter()
                .enumerate()
                .filter_map(|(idx, rendered)| {
                    if (filter.is_empty() || filter.contains(&idx)) && (show_mangled || !rendered.contains('$')) {
                        Some((idx, rendered))
                    } else {
                        None
                    }
                })
                .collect();
            if res.is_empty() {
                println!("no interned strings matched indices {}", filter.iter().join(", "));
                return Ok(());
            }
            if let SortOrder::ByName = sort_order {
                res.sort_by(|(_, rendered_a), (_, rendered_b)| rendered_a.cmp(rendered_b));
            } else {
                res.sort_by(|(index_a, _), (index_b, _)| index_a.cmp(index_b));
            }
            let mut table = Table::new();
            table.set_titles(Row::new(vec!["index", "rendered"].into_iter().map(Cell::new).collect()));
            table.set_format(*format::consts::FORMAT_NO_LINESEP_WITH_TITLE);
            for (idx, rendered) in &res {
                table.add_row(string_row(idx.to_string().as_str(), rendered, pick_color(rendered)));
            }
            table.printstd();
        },
    }
    Ok(())
}

fn intern_dotted(dar_path: &str, show_mangled: bool, sort_order: &SortOrder, filter: &[usize]) -> Result<()> {
    let dar = DarFile::from_file(dar_path)?;
    match dar.main.payload.package {
        DamlLfPackage::V2(package) => {
            let mut res: Vec<_> = package
                .interned_dotted_names
                .iter()
                .enumerate()
                .filter_map(|(idx, dt)| {
                    if filter.is_empty() || filter.contains(&idx) {
                        let segments = dt
                            .segments_interned_str
                            .iter()
                            .map(|&i| format!("{}({})", package.interned_strings[i as usize], i))
                            .join(".");
                        let rendered =
                            dt.segments_interned_str.iter().map(|&i| &package.interned_strings[i as usize]).join(".");
                        if show_mangled || !rendered.contains('$') {
                            Some((idx, rendered, segments))
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                })
                .collect();
            if res.is_empty() {
                println!("no interned dotted names matched indices {}", filter.iter().join(", "));
                return Ok(());
            }
            if let SortOrder::ByName = sort_order {
                res.sort_by(|(_, rendered_a, _), (_, rendered_b, _)| rendered_a.cmp(rendered_b));
            } else {
                res.sort_by(|(index_a, ..), (index_b, ..)| index_a.cmp(index_b));
            }
            let mut table = Table::new();
            table.set_titles(Row::new(vec!["index", "rendered", "segments"].into_iter().map(Cell::new).collect()));
            table.set_format(*format::consts::FORMAT_NO_LINESEP_WITH_TITLE);
            for (idx, rendered, segments) in &res {
                table.add_row(dotted_row(idx.to_string().as_str(), rendered, segments, pick_color(rendered)));
            }
            table.printstd();
        },
    }
    Ok(())
}

fn string_row(idx: &str, rendered: &str, color: color::Color) -> Row {
    Row::new(vec![cell(idx, color), cell(rendered, color)])
}

fn dotted_row(idx: &str, rendered: &str, segments: &str, color: color::Color) -> Row {
    Row::new(vec![cell(idx, color), cell(rendered, color), cell(segments, color)])
}

fn cell(data: &str, color: color::Color) -> Cell {
    Cell::new(data).with_style(Attr::Bold).with_style(Attr::ForegroundColor(color))
}

fn pick_color(data: &str) -> Color {
    if data.contains('$') {
        color::BLUE
    } else {
        color::WHITE
    }
}
