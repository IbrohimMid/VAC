#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use std::collections::HashSet;

    #[test]
    fn test_keymap_uniqueness_contract() {
        let source = include_str!("event.rs");
        let mut seen = HashSet::new();

        for line in source.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with("KeyCode::") {
                let pattern = if trimmed.contains("=>") {
                    trimmed.split("=>").next().unwrap().trim().to_string()
                } else {
                    trimmed.split('{').next().unwrap().trim().to_string()
                };
                let normalized = pattern.split_whitespace().collect::<Vec<_>>().join(" ");
                // Ignore wildcard or variable binding like `KeyCode::Char(c)`
                if normalized.contains("KeyCode::Char(c)") {
                    continue;
                }
                assert!(seen.insert(normalized.clone()), "Duplicate keymap found in event.rs: {}", normalized);
            }
        }
    }

    #[test]
    fn test_command_dispatch_uniqueness_contract() {
        let commands = crate::services::helper_block::vac_commands();
        let mut seen = HashSet::new();
        for cmd in commands {
            assert!(seen.insert(cmd.command.clone()), "Duplicate command found: {}", cmd.command);
        }
    }
}
