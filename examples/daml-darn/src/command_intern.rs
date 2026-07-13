use anyhow::Result;
use itertools::Itertools;
use prettytable::color::Color;
use prettytable::format;
use prettytable::{color, Attr, Cell, Row, Table};

use daml::lf::{DamlLfPackage, DarFile};

/// Ordering for the intern-table output.
pub enum SortOrder {
    ByIndex,
    ByName,
}

/// Print the LF2 `interned_strings` table of the DAR's main package.
pub fn intern_string(dar_path: &str, show_mangled: bool, sort_order: &SortOrder, filter: &[usize]) -> Result<()> {
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

/// Print the LF2 `interned_dotted_names` table of the DAR's main
/// package, with each dotted name resolved into a `Foo(23).Bar(47)`
/// index-annotated form.
pub fn intern_dotted(dar_path: &str, show_mangled: bool, sort_order: &SortOrder, filter: &[usize]) -> Result<()> {
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
