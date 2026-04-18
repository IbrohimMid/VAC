use crate::ToolCall;

pub fn approval_preview(tc: &ToolCall) -> String {
    let mut parts = vec![tc.function.name.clone()];
    let paths = extract_paths(&tc.function.arguments);
    if !paths.is_empty() {
        parts.push(format!("files={}", paths.join(",")));
    }
    parts.join(" ")
}

fn extract_paths(arguments: &str) -> Vec<String> {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(arguments) else {
        return vec![];
    };
    let mut out: Vec<String> = Vec::new();
    collect_paths(&value, None, &mut out);
    out.sort();
    out.dedup();
    out.truncate(3);
    out
}

fn collect_paths(value: &serde_json::Value, key: Option<&str>, out: &mut Vec<String>) {
    match value {
        serde_json::Value::String(s) => {
            if key.is_some_and(is_path_key) {
                out.push(s.clone());
            }
        }
        serde_json::Value::Array(arr) => {
            for v in arr {
                collect_paths(v, key, out);
            }
        }
        serde_json::Value::Object(map) => {
            for (k, v) in map {
                collect_paths(v, Some(k.as_str()), out);
            }
        }
        _ => {}
    }
}

fn is_path_key(k: &str) -> bool {
    matches!(
        k,
        "path"
            | "file_path"
            | "filepath"
            | "filename"
            | "target_file"
            | "target_path"
            | "source_file"
            | "source_path"
    )
}
