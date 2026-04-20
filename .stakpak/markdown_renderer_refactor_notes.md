# Markdown Renderer Refactoring Progress

## Status: Phase 1 Complete (1450 → 1215 lines, 16% reduction)

### What Was Extracted

**layout.rs** (159 lines)
- `display_width()` — Unicode-aware display width calculation
- `char_display_width()` — Single character width with emoji support
- `wrap_text()` — Text wrapping respecting terminal width
- `break_long_word()` — Break words longer than max width
- `truncate_text()` — Truncate with ellipsis
- `strip_markdown_for_table()` — Clean markdown from table cells

**inline.rs** (122 lines)
- `parse_inline_formatting()` — Parse bold (**) and inline code (`)
- `parse_image()` — Extract image syntax: ![alt](url)
- `parse_link()` — Extract link syntax: [text](url)

### What Remains in renderer.rs (1215 lines)

#### Small Methods (easy to understand)
- Constructor: `new()`, `with_width()`
- Input prep: `preprocess_input()`, `strip_line_number()`
- Terminal: `get_terminal_width()`
- Detection helpers: `has_inline_formatting_fast()`, `has_simple_formatting()`

#### Medium Methods (reasonable complexity)
- Line parsers (lines 137-504): `parse_line_optimized()`, `parse_heading_fast()`, `parse_simplified_link()`, `parse_code_block_safe()`, `parse_list_item_safe()`, `parse_quote_safe()`, `parse_callout()`, `parse_numbered_list()` — **365 lines total**
- Table parsing: `parse_table_safe()` — **102 lines**
- Public API: `parse_markdown()`, `render_to_lines()` — **30 lines**

#### Large Methods (extraction would be beneficial)
- **`component_to_lines()`** (lines 634–1121): **488 lines**  
  This is a massive recursive function that converts MarkdownComponent enum variants to ratatui Lines with styling. Each match arm handles different component types (H1-H6, bold, lists, code blocks, tables, etc.).
  
- **`wrap_mixed_content()`** (lines 1122–1178): **57 lines**
  Helper for wrapping formatted spans while preserving styling
  
- **`try_syntax_highlighting()`** (lines 1179–1215): **37 lines**
  Attempts syntax highlighting on code blocks

---

## Why We Can't Get Much Lower Without Major Refactoring

### Problem 1: Rust `impl` Block Limitation
In Rust, you cannot split a single `impl Block` across multiple files. The entire impl must stay together. 

**What we CAN do:**
✓ Extract pure helper functions to submodules (what we did ✓)
✓ Create wrapper methods that call into extracted modules (what we did ✓)

**What we CAN'T easily do:**
✗ Move individual methods to a different impl block in a submodule (requires mod struct pattern)
✗ Create a partial impl in a submodule

### Problem 2: `component_to_lines()` Interdependencies
The 488-line `component_to_lines()` method:
- Is deeply recursive (calls itself for list items, nested components)
- Uses `self.style` in every pattern match arm
- Calls `self.parse_markdown()` for nested markdown (code blocks)
- Calls `self.wrap_mixed_content()` for complex formatting

Extracting this to a free function would require:
- Passing `&MarkdownStyle` to every recursive call
- Passing `&self` for `parse_markdown()` access, making it `&MarkdownRenderer`
- Significant complexity increase for marginal benefit

---

## Paths to Get Under 600 Lines

### Path A: Modular Sub-impl (Medium Effort, Good Results)
Create a `ComponentRenderer` helper struct in a `component_renderer.rs` module:

```rust
// In component_renderer.rs
pub struct ComponentRenderer<'a> {
    style: &'a MarkdownStyle,
    renderer: &'a MarkdownRenderer,
}

impl<'a> ComponentRenderer<'a> {
    pub fn to_lines(&self, component: MarkdownComponent) -> Vec<Line<'static>> {
        // recursive component_to_lines logic here
    }
}
```

**Pros:** 
- Cleaner separation of concerns
- Reduces renderer.rs to ~650 lines
- Testable independently

**Cons:**
- Lifetime complexity
- Needs borrowing from MarkdownRenderer

### Path B: Trait-Based Rendering (High Effort, Very Clean)
Create a `Renderer` trait and impl it on `MarkdownRenderer`:

```rust
trait ComponentRenderer {
    fn render_component(&self, component: MarkdownComponent) -> Vec<Line<'static>>;
}

impl ComponentRenderer for MarkdownRenderer {
    // ... in a separate module
}
```

**Pros:**
- Fully modular
- Could have multiple renderers (e.g., HTML, PlainText)
- Very testable

**Cons:**
- Significant refactoring
- Lifetime/borrowing complexity
- Overkill if only one renderer needed

### Path C: Aggressive Simplification (Medium Effort)
- Extract table rendering to separate TableRenderer struct
- Move code block handling to CodeBlockRenderer struct  
- Reduce `component_to_lines` to ~250 lines by delegating to these

**Pros:**
- Focused refactoring
- Gets renderer.rs to ~800-900 lines
- Minimal complexity increase

**Cons:**
- Still requires multiple new files
- More testing needed

---

## Current File Sizes Summary

```
122 crates/vac_tui_runtime/src/services/markdown_renderer/inline.rs
159 crates/vac_tui_runtime/src/services/markdown_renderer/layout.rs
333 crates/vac_tui_runtime/src/services/markdown_renderer/mod.rs
245 crates/vac_tui_runtime/src/services/markdown_renderer/style.rs
1215 crates/vac_tui_runtime/src/services/markdown_renderer/renderer.rs (was 1450)
----
2074 total (was 2239)
```

## Test Status
✓ All 6 markdown tests passing
✓ No breaking changes to public API
✓ Compilation successful with no errors

---

## Next Steps (if continuing)

1. **Quick win** (15 min): Extract `wrap_mixed_content()` and `try_syntax_highlighting()` to helpers (~94 lines)
   - Would get renderer.rs to ~1120 lines

2. **Medium effort** (1-2 hours): Path C (ComponentRenderer struct)
   - Could get to ~800-900 lines with manageable complexity

3. **Best long-term** (3-4 hours): Path B (Trait-based design)
   - Enables multiple renderer backends
   - Gets below 600 lines
   - More maintainable architecture
