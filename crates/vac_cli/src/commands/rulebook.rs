//! `vac rulebook` — manage and validate rulebooks.

use std::path::PathBuf;
use vac_core::rulebook::{RulebookLoader, validate_rulebooks};

pub async fn execute_list(project_root: PathBuf) -> anyhow::Result<()> {
    let books = RulebookLoader::load_all(&project_root, &[]);
    if books.is_empty() {
        println!("No rulebooks found.");
        println!("  Create .vac/rules.toml or .vac/rulebooks/*.toml");
        return Ok(());
    }
    println!("Loaded {} rulebook(s):\n", books.len());
    for book in &books {
        let constraint_count = book.all_constraints().len();
        println!("  📋 {} (priority: {}, {} constraints)", book.id, book.priority, constraint_count);
        if let Some(ref name) = book.name {
            println!("     {name}");
        }
        for c in book.all_constraints().iter().take(5) {
            let marker = if c.severity == "block" { "🔴" } else { "⚠️" };
            println!("     {marker} [{}] {}", c.id, c.description);
        }
        if constraint_count > 5 {
            println!("     ... and {} more", constraint_count - 5);
        }
    }
    Ok(())
}

pub async fn execute_validate(project_root: PathBuf) -> anyhow::Result<()> {
    let books = RulebookLoader::load_all(&project_root, &[]);
    if books.is_empty() {
        println!("✓ No rulebooks to validate.");
        return Ok(());
    }
    let result = validate_rulebooks(&books);
    if result.errors.is_empty() && result.warnings.is_empty() {
        println!("✓ All {} rulebook(s) valid.", books.len());
        return Ok(());
    }
    for err in &result.errors {
        println!("✗ {err}");
    }
    for warn in &result.warnings {
        println!("⚠ {warn}");
    }
    if !result.is_valid() {
        anyhow::bail!("Rulebook validation failed with {} error(s)", result.errors.len());
    }
    Ok(())
}
