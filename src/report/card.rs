#![cfg(feature = "card")]

use super::Report;
use anyhow::Result;
use std::path::Path;

pub fn score_card(report: &Report) -> String {
    // Placeholder until the card milestone.
    format!(
        "<svg xmlns='http://www.w3.org/2000/svg' width='1200' height='630'>\
         <text x='60' y='100'>slop {:.1}%</text></svg>",
        report.score.slop_percent
    )
}

pub fn history_card(report: &crate::history::HistoryReport) -> String {
    // Placeholder until the card milestone.
    format!(
        "<svg xmlns='http://www.w3.org/2000/svg' width='1200' height='630'>\
         <text x='60' y='100'>{} snapshots</text></svg>",
        report.series.len()
    )
}

pub fn write_card(svg: &str, dir: &Path) -> Result<()> {
    std::fs::write(dir.join("dupehound-card.svg"), svg)?;
    Ok(())
}
