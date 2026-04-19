use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;
use tokio::io::AsyncReadExt;
use tokio::process::Command;

#[derive(Debug, Clone)]
pub struct ContextCrawler {
    pub per_command_timeout: Duration,
    pub max_output_bytes: usize,
    pub max_files: usize,
}

impl Default for ContextCrawler {
    fn default() -> Self {
        Self {
            per_command_timeout: Duration::from_millis(300),
            max_output_bytes: 16 * 1024,
            max_files: 25,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AgentContextSnapshot {
    pub cwd: String,
    pub git: Option<GitSnapshot>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GitSnapshot {
    pub branch: Option<String>,
    pub head: Option<String>,
    pub last_commit: Option<String>,
    pub changes: GitChangesSummary,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct GitChangesSummary {
    pub staged: usize,
    pub unstaged: usize,
    pub untracked: usize,
    pub conflicted: usize,
    pub total_files: usize,
    pub files: Vec<GitFileStatus>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GitFileStatus {
    pub status: String,
    pub path: String,
}

impl AgentContextSnapshot {
    pub fn to_prompt_block(&self) -> String {
        self.to_prompt_block_with_limit(3000)
    }

    pub fn to_prompt_block_with_limit(&self, max_chars: usize) -> String {
        let mut out = String::new();
        out.push_str("Workspace Context:\n");
        out.push_str(&format!("- CWD: {}\n", self.cwd));

        match &self.git {
            None => {
                out.push_str("- Git: unavailable\n");
            }
            Some(g) => {
                let branch = g.branch.as_deref().unwrap_or("?");
                let head = g.head.as_deref().unwrap_or("?");
                out.push_str(&format!("- Git: {branch} @ {head}\n"));
                if let Some(lc) = g.last_commit.as_deref() {
                    if !lc.trim().is_empty() {
                        out.push_str(&format!("- Last commit: {}\n", lc.trim()));
                    }
                }
                let c = &g.changes;
                out.push_str(&format!(
                    "- Changes: staged {}, unstaged {}, untracked {}, conflicted {}\n",
                    c.staged, c.unstaged, c.untracked, c.conflicted
                ));
                if !c.files.is_empty() {
                    out.push_str("- Files:\n");
                    for f in &c.files {
                        out.push_str(&format!("  - {} {}\n", f.status, f.path));
                    }
                    if c.total_files > c.files.len() {
                        out.push_str(&format!(
                            "  - ... ({} more)\n",
                            c.total_files - c.files.len()
                        ));
                    }
                }
            }
        }

        if out.len() > max_chars {
            let boundary = char_boundary_at_or_before(&out, max_chars);
            out.truncate(boundary);
            if !out.ends_with('\n') {
                out.push('\n');
            }
        }

        out
    }
}

impl ContextCrawler {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn crawl(&self, root: &Path) -> AgentContextSnapshot {
        let cwd_path = tokio::fs::canonicalize(root)
            .await
            .unwrap_or_else(|_| root.to_path_buf());
        let cwd = cwd_path.display().to_string();

        let inside = self
            .run_git(root.to_path_buf(), &["rev-parse", "--is-inside-work-tree"])
            .await
            .map(|s| s.trim() == "true")
            .unwrap_or(false);

        if !inside {
            return AgentContextSnapshot { cwd, git: None };
        }

        let root = root.to_path_buf();
        let (branch, head, last_commit, status) = tokio::join!(
            self.run_git(root.clone(), &["rev-parse", "--abbrev-ref", "HEAD"]),
            self.run_git(root.clone(), &["rev-parse", "--short", "HEAD"]),
            self.run_git(root.clone(), &["log", "-1", "--pretty=format:%h %s"]),
            self.run_git(root.clone(), &["status", "--porcelain=v1"])
        );

        let changes =
            GitChangesSummary::from_porcelain(status.as_deref().unwrap_or(""), self.max_files);

        AgentContextSnapshot {
            cwd,
            git: Some(GitSnapshot {
                branch: branch
                    .filter(|s| !s.trim().is_empty())
                    .map(|s| s.trim().to_string()),
                head: head
                    .filter(|s| !s.trim().is_empty())
                    .map(|s| s.trim().to_string()),
                last_commit: last_commit
                    .filter(|s| !s.trim().is_empty())
                    .map(|s| s.trim().to_string()),
                changes,
            }),
        }
    }

    async fn run_git(&self, cwd: PathBuf, args: &[&str]) -> Option<String> {
        let mut cmd = Command::new("git");
        cmd.args(args)
            .current_dir(&cwd)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let mut child = cmd.spawn().ok()?;
        let mut stdout = child.stdout.take()?;
        let mut stderr = child.stderr.take()?;

        let max = self.max_output_bytes;
        let stdout_task = tokio::spawn(async move { read_limited(&mut stdout, max).await });
        let stderr_task = tokio::spawn(async move { read_limited(&mut stderr, max).await });

        let status = tokio::time::timeout(self.per_command_timeout, child.wait()).await;
        let _status = match status {
            Ok(Ok(s)) => s,
            Ok(Err(_)) => return None,
            Err(_) => {
                let _ = child.kill().await;
                let _ = child.wait().await;
                return None;
            }
        };

        let stdout = stdout_task.await.ok().flatten().unwrap_or_default();
        let stderr = stderr_task.await.ok().flatten().unwrap_or_default();

        let stdout = String::from_utf8_lossy(&stdout).trim().to_string();
        if !stdout.is_empty() {
            return Some(stdout);
        }
        let stderr = String::from_utf8_lossy(&stderr).trim().to_string();
        if !stderr.is_empty() {
            return Some(stderr);
        }
        None
    }
}

impl GitChangesSummary {
    pub fn from_porcelain(porcelain: &str, max_files: usize) -> Self {
        let mut out = GitChangesSummary::default();
        for line in porcelain.lines() {
            if line.is_empty() {
                continue;
            }
            if line.starts_with("## ") {
                continue;
            }
            let Some((status, path)) = parse_porcelain_entry(line) else {
                continue;
            };

            out.total_files += 1;
            if status == "??" {
                out.untracked += 1;
            } else if is_conflict_status(status) {
                out.conflicted += 1;
            } else {
                let staged = status.as_bytes()[0] != b' ';
                let unstaged = status.as_bytes()[1] != b' ';
                if staged {
                    out.staged += 1;
                }
                if unstaged {
                    out.unstaged += 1;
                }
            }
            if out.files.len() < max_files {
                out.files.push(GitFileStatus {
                    status: status.to_string(),
                    path: path.to_string(),
                });
            }
        }
        out
    }
}

async fn read_limited<R: tokio::io::AsyncRead + Unpin>(r: &mut R, max: usize) -> Option<Vec<u8>> {
    let mut buf = Vec::new();
    let mut chunk = [0u8; 1024];
    loop {
        if buf.len() >= max {
            break;
        }
        let read_cap = (max - buf.len()).min(chunk.len());
        match r.read(&mut chunk[..read_cap]).await {
            Ok(0) => break,
            Ok(n) => buf.extend_from_slice(&chunk[..n]),
            Err(_) => return None,
        }
    }
    Some(buf)
}

fn is_conflict_status(code: &str) -> bool {
    code == "AA" || code == "DD" || code.as_bytes().contains(&b'U')
}

fn parse_porcelain_entry(line: &str) -> Option<(&str, &str)> {
    let bytes = line.as_bytes();
    if bytes.len() < 4 || bytes[2] != b' ' {
        return None;
    }

    let status = std::str::from_utf8(&bytes[..2]).ok()?;
    let raw_path = line.get(3..)?;
    if raw_path.is_empty() {
        return None;
    }

    let is_rename_or_copy = status.as_bytes().iter().any(|b| matches!(b, b'R' | b'C'));
    let path = if is_rename_or_copy {
        raw_path
            .rsplit_once(" -> ")
            .map(|(_, renamed)| renamed)
            .filter(|renamed| !renamed.is_empty())
            .unwrap_or(raw_path)
    } else {
        raw_path
    };

    Some((status, path))
}

fn char_boundary_at_or_before(s: &str, limit: usize) -> usize {
    let mut boundary = limit.min(s.len());
    while boundary > 0 && !s.is_char_boundary(boundary) {
        boundary -= 1;
    }
    boundary
}

pub fn inject_workspace_context(messages: &mut Vec<vil_llm::provider::Message>, prompt: String) {
    if prompt.trim().is_empty() {
        return;
    }
    if messages.iter().any(|m| {
        m.role == vil_llm::provider::Role::System && m.content.contains("Workspace Context:")
    }) {
        return;
    }
    let insert_at = messages
        .iter()
        .position(|m| m.role != vil_llm::provider::Role::System)
        .unwrap_or(messages.len());
    messages.insert(insert_at, vil_llm::provider::Message::system(prompt));
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn serializer_emits_stable_fields() {
        let snap = AgentContextSnapshot {
            cwd: "/tmp/project".to_string(),
            git: Some(GitSnapshot {
                branch: Some("main".to_string()),
                head: Some("abc123".to_string()),
                last_commit: Some("abc123 feat: test".to_string()),
                changes: GitChangesSummary {
                    staged: 1,
                    unstaged: 2,
                    untracked: 3,
                    conflicted: 0,
                    total_files: 2,
                    files: vec![
                        GitFileStatus {
                            status: "M ".to_string(),
                            path: "src/lib.rs".to_string(),
                        },
                        GitFileStatus {
                            status: "??".to_string(),
                            path: "README.md".to_string(),
                        },
                    ],
                },
            }),
        };

        let s = snap.to_prompt_block_with_limit(10_000);
        assert!(s.contains("Workspace Context:"));
        assert!(s.contains("CWD: /tmp/project"));
        assert!(s.contains("Git: main @ abc123"));
        assert!(s.contains("Changes: staged 1, unstaged 2, untracked 3, conflicted 0"));
        assert!(s.contains("M  src/lib.rs"));
        assert!(s.contains("?? README.md"));
    }

    #[tokio::test]
    async fn fallback_non_git_repo() {
        let dir = tempdir().unwrap();
        let crawler = ContextCrawler::new();
        let snap = crawler.crawl(dir.path()).await;
        assert!(snap.git.is_none());
        let s = snap.to_prompt_block();
        assert!(s.contains("Git: unavailable"));
    }

    #[test]
    fn parse_porcelain_counts_and_limits() {
        let porcelain = " M src/a.rs\nM  src/b.rs\nUU src/c.rs\n?? src/d.rs\n?? src/e.rs\n";
        let sum = GitChangesSummary::from_porcelain(porcelain, 2);
        assert_eq!(sum.total_files, 5);
        assert_eq!(sum.staged, 1);
        assert_eq!(sum.unstaged, 1);
        assert_eq!(sum.untracked, 2);
        assert_eq!(sum.conflicted, 1);
        assert_eq!(sum.files.len(), 2);
    }

    #[test]
    fn parse_porcelain_rename_keeps_destination_path_for_multibyte_names() {
        let porcelain = "R  old/naïve.rs -> new/你好.rs\n";
        let sum = GitChangesSummary::from_porcelain(porcelain, 5);
        assert_eq!(sum.total_files, 1);
        assert_eq!(sum.staged, 1);
        assert_eq!(sum.files.len(), 1);
        assert_eq!(sum.files[0].status, "R ");
        assert_eq!(sum.files[0].path, "new/你好.rs");
    }

    #[test]
    fn prompt_block_truncation_never_panics_on_multibyte_boundaries() {
        let snap = AgentContextSnapshot {
            cwd: "/tmp/你".to_string(),
            git: None,
        };
        let limit = "Workspace Context:\n- CWD: /tmp/".len() + 1;
        let rendered = std::panic::catch_unwind(|| snap.to_prompt_block_with_limit(limit))
            .expect("truncation should not panic on multibyte boundaries");
        assert!(rendered.len() <= limit);
        assert!(rendered.ends_with('\n'));
    }
}
