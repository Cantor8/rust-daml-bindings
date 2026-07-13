use anyhow::Result;
use daml::lf::DarFile;
use itertools::Itertools;
use prettytable::format;
use prettytable::{color, Attr, Cell, Row, Table};

/// Print a table of every package in the DAR at `dar_path`. The main
/// package (the one the manifest points at) is coloured green;
/// dependencies white.
pub fn show_package(dar_path: &str) -> Result<()> {
    let dar = DarFile::from_file(dar_path)?;
    Ok(dar.apply(|archive| {
        let mut table = Table::new();
        table.set_titles(Row::new(vec!["name", "version", "package_id", "lf"].into_iter().map(Cell::new).collect()));
        table.set_format(*format::consts::FORMAT_NO_LINESEP_WITH_TITLE);
        archive.packages().sorted_by_key(|&p| p.package_id()).for_each(|package| {
            let name = package.name();
            let version = package.version().unwrap_or("n/a");
            let package_id = package.package_id();
            let language_version = &package.language_version().to_string();
            let color = if package_id == dar.main.hash { color::GREEN } else { color::WHITE };
            table.add_row(row(name, version, package_id, language_version, color));
        });
        table.printstd();
    })?)
}

fn row(name: &str, version: &str, package_id: &str, language_version: &str, color: color::Color) -> Row {
    Row::new(vec![cell(name, color), cell(version, color), cell(package_id, color), cell(language_version, color)])
}

fn cell(data: &str, color: color::Color) -> Cell {
    Cell::new(data).with_style(Attr::Bold).with_style(Attr::ForegroundColor(color))
}
