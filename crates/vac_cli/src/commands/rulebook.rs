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

pub async fn execute_apply(project_root: PathBuf, path: PathBuf) -> anyhow::Result<()> {
    if !path.exists() {
        anyhow::bail!("File not found: {}", path.display());
    }

    let content = std::fs::read_to_string(&path)?;

    // Parse YAML frontmatter between --- delimiters
    let rulebook_id = if content.starts_with("---") {
        let end = content[3..].find("---").map(|i| i + 3);
        if let Some(end_idx) = end {
            let frontmatter = &content[3..end_idx];
            frontmatter
                .lines()
                .find_map(|l| l.strip_prefix("id:").map(|v| v.trim().to_string()))
        } else {
            None
        }
    } else {
        None
    };

    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("rulebook");
    let id = rulebook_id.unwrap_or_else(|| stem.to_string());

    let dest_dir = project_root.join(".vac/rulebooks");
    std::fs::create_dir_all(&dest_dir)?;
    let dest = dest_dir.join(format!("{id}.md"));

    // Rollback safety: snapshot existing rulebook before overwriting
    if dest.exists() {
        let backup = dest_dir.join(format!("{id}.md.bak"));
        std::fs::copy(&dest, &backup)?;
        println!("  ↩ Previous rulebook backed up to {}", backup.display());
    }

    std::fs::copy(&path, &dest)?;
    println!("✓ Applied rulebook '{id}' → {}", dest.display());
    println!("  Run `vac rulebook list` to verify.");
    Ok(())
}
