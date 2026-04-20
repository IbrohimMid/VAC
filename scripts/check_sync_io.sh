#!/usr/bin/env bash
set -eo pipefail

# Guardrail to prevent sync I/O in async-sensitive paths
# Supports baseline-ratcheting to prevent new regressions without breaking existing code.

CRATES=("crates/vac_cli" "crates/vac_runtime" "crates/vac_tui_runtime" "crates/vil_rag" "crates/vac_core" "crates/vil_swarm" "crates/vac_session_control" "crates/vac_trace" "crates/vil_knowledge")
STRICT_CRATES=()

PATTERNS=("stdin\(\)\.read_line" "blocking_read\(" "blocking_write\(" "std::fs::write\(" "fs::write\(" "std::fs::read_to_string\(" "fs::read_to_string\(" "std::fs::read_dir\(" "fs::read_dir\(" "std::fs::remove_dir_all\(" "fs::remove_dir_all\(" "std::fs::create_dir_all\(" "fs::create_dir_all\(" "std::fs::copy\(" "fs::copy\(" "std::fs::canonicalize\(" "fs::canonicalize\(" "std::thread::sleep\(" "blocking_lock" "use std::fs\\b")
PATTERN_STR=$(IFS="|"; echo "${PATTERNS[*]}")

BASELINE_FILE="scripts/sync_io_baseline.txt"
DUMP_BASELINE=0

if [ "$1" == "--dump-baseline" ]; then
    DUMP_BASELINE=1
    echo "Dumping baseline to $BASELINE_FILE..."
    > "$BASELINE_FILE"
fi

echo "Running sync I/O regression guardrails..."
FAILED=0
ALL_HITS=""

for CRATE in "${CRATES[@]}"; do
    if [ ! -d "$CRATE" ]; then
        continue
    fi

    # Use ripgrep or standard grep
    # Exclude test files and known sync wrapper modules
    if command -v rg >/dev/null 2>&1; then
        MATCHES=$(rg -n "$PATTERN_STR" "$CRATE" \
            -g '!*tests*' -g '!*test*' -g '!**/vac_cli/src/io.rs' -g '!**/vac_runtime/src/state_writer.rs' -g '!**/vil_swarm/src/sandbox.rs' || true)
    else
        MATCHES=$(grep -rE "$PATTERN_STR" "$CRATE" \
            --exclude-dir=tests --exclude=*test* --exclude=*io.rs --exclude=*state_writer.rs --exclude=*sandbox.rs || true)
    fi

    # Filter out matches inside inline test modules (heuristic: lines after mod tests {)
    if [ -n "$MATCHES" ]; then
        FILTERED=$(echo "$MATCHES" | grep -v "tokio::fs" | grep -v "allow_sync_io" || true)

        if [ -n "$FILTERED" ]; then
            if [ "$DUMP_BASELINE" -eq 1 ]; then
                echo "$FILTERED" >> "$BASELINE_FILE"
            fi
            ALL_HITS+="$FILTERED"$'\n'

            # Check if this crate is strictly enforced
            IS_STRICT=0
            for SC in "${STRICT_CRATES[@]}"; do
                if [ "$CRATE" == "$SC" ]; then
                    IS_STRICT=1
                    break
                fi
            done

            if [ "$IS_STRICT" -eq 1 ] && [ "$DUMP_BASELINE" -eq 0 ]; then
                echo "❌ STRICT ERROR: Found blocking sync I/O in latency-critical crate '$CRATE':"
                echo "$FILTERED"
                echo "Please wrap in 'tokio::task::spawn_blocking'."
                FAILED=1
            fi
        fi
    fi
done

if [ "$DUMP_BASELINE" -eq 1 ]; then
    echo "Baseline dumped successfully."
    # Sort baseline to normalize
    sort "$BASELINE_FILE" -o "$BASELINE_FILE"
    exit 0
fi

# Baseline Ratchet Check
if [ -f "$BASELINE_FILE" ]; then
    SORTED_HITS=$(echo -n "$ALL_HITS" | grep -v '^$' | sort || true)
    BASELINE=$(cat "$BASELINE_FILE" | sort)

    NEW_HITS=$(comm -23 <(echo "$SORTED_HITS") <(echo "$BASELINE"))

    if [ -n "$NEW_HITS" ]; then
        echo "❌ BASELINE REGRESSION: Found NEW blocking sync I/O:"
        echo "$NEW_HITS"
        echo "Please wrap in 'tokio::task::spawn_blocking'. If this is unavoidable, update the baseline with '--dump-baseline'."
        FAILED=1
    fi
else
    echo "⚠️  No baseline file found at $BASELINE_FILE. Run with '--dump-baseline' to create one."
fi

if [ $FAILED -ne 0 ]; then
    exit 1
fi

echo "✅ Sync I/O guardrail checks passed."
exit 0
