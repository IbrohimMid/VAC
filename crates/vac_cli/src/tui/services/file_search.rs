use ignore::WalkBuilder;
use nucleo_matcher::{
    Matcher, Utf32Str,
    pattern::{AtomKind, CaseMatching, Normalization, Pattern},
};
use std::cmp::Reverse;
use std::collections::BinaryHeap;
use std::path::Path;

pub fn build_file_index(project_root: &Path) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let walker = WalkBuilder::new(project_root)
        .hidden(false)
        .follow_links(false)
        .git_ignore(true)
        .git_exclude(true)
        .build();

    for entry in walker {
        let Ok(entry) = entry else { continue };
        let Some(ft) = entry.file_type() else { continue };
        if !ft.is_file() {
            continue;
        }
        let path = entry.path();
        let rel = path.strip_prefix(project_root).unwrap_or(path);
        let s = rel.to_string_lossy().to_string();
        if s.starts_with(".git/")
            || s.starts_with("target/")
            || s.starts_with(".vac/")
            || s.starts_with(".trae/")
        {
            continue;
        }
        out.push(s);
    }

    out.sort();
    out.dedup();
    out
}

pub fn fuzzy_search_files(query: &str, files: &[String], max_matches: usize) -> Vec<String> {
    let q = query.trim();
    if q.is_empty() {
        return files.iter().take(max_matches).cloned().collect();
    }

    let pattern = Pattern::new(
        q,
        CaseMatching::Smart,
        Normalization::Smart,
        AtomKind::Fuzzy,
    );
    let mut best = BestMatchesList::new(max_matches, pattern, Matcher::new(nucleo_matcher::Config::DEFAULT));
    for f in files {
        best.insert(f);
    }
    best.get_sorted_matches()
}

#[derive(Debug)]
struct BestMatchesList {
    max_count: usize,
    pattern: Pattern,
    matcher: Matcher,
    binary_heap: BinaryHeap<Reverse<(u32, String)>>,
    utf32buf: Vec<char>,
}

impl BestMatchesList {
    fn new(max_count: usize, pattern: Pattern, matcher: Matcher) -> Self {
        Self {
            max_count,
            pattern,
            matcher,
            binary_heap: BinaryHeap::new(),
            utf32buf: Vec::new(),
        }
    }

    fn insert(&mut self, file_path: &str) {
        let haystack: Utf32Str<'_> = Utf32Str::new(file_path, &mut self.utf32buf);
        if let Some(score) = self.pattern.score(haystack, &mut self.matcher) {
            if self.binary_heap.len() < self.max_count {
                self.binary_heap
                    .push(Reverse((score, file_path.to_string())));
            } else if let Some(min_element) = self.binary_heap.peek()
                && score > min_element.0 .0
            {
                self.binary_heap.pop();
                self.binary_heap
                    .push(Reverse((score, file_path.to_string())));
            }
        }
    }

    fn get_sorted_matches(&mut self) -> Vec<String> {
        let mut sorted_matches: Vec<(u32, String)> = self
            .binary_heap
            .drain()
            .map(|Reverse((score, path))| (score, path))
            .collect();
        sorted_matches.sort_by(|a, b| match b.0.cmp(&a.0) {
            std::cmp::Ordering::Equal => a.1.cmp(&b.1),
            other => other,
        });
        sorted_matches
            .into_iter()
            .map(|(_, path)| path)
            .collect()
    }
}

