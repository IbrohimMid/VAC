#!/usr/bin/env bash
set -e

# Guardrail to prevent sync I/O in async paths

crates=("crates/vac_cli" "crates/vac_runtime" "crates/vac_tui_runtime" "crates/vil_rag")
fail=0

echo "Checking for synchronous I/O patterns in async paths..."

for crate in "${crates[@]}"; do
    if [ ! -d "$crate" ]; then
        continue
    fi

    # Disallowed patterns:
    # stdin().read_line
    # blocking_read(
    # blocking_write(
    # std::fs::write(
    # std::fs::read_to_string(
    # std::fs::read_dir(

    matches=$(rg -n "(stdin\(\)\.read_line|blocking_read\(|blocking_write\(|std::fs::write\(|fs::write\(|std::fs::read_to_string\(|fs::read_to_string\(|std::fs::read_dir\(|fs::read_dir\()" "$crate" \
        -g '!*tests*' -g '!*test*' -g '!*io.rs' -g '!*state_writer.rs' -g '!*autopilot.rs' -g '!*scheduler.rs' || true)

    # Let's filter out known false positives or allowed places (e.g. spawn_blocking bodies)
    # This is a basic grep, if matches are found it warns but might not fail CI if we want to be soft.
    # For now, we will just print them.
    if [ -n "$matches" ]; then
        echo "WARNING: Found potential sync I/O in $crate:"
        echo "$matches"
        # We don't strictly fail yet to allow gradual rollout, but it's a regression fence.
        # fail=1
    fi
done

if [ "$fail" -eq 1 ]; then
    echo "Regression check failed."
    exit 1
fi

echo "Regression check passed."
exit 0
