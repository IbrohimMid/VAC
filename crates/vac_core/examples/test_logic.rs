fn strip_leading_tokens<'a>(tokens: &'a [String], value_flags: &[&str]) -> &'a [String] {
    let mut idx = 0usize;
    while idx < tokens.len() {
        let token = tokens[idx].as_str();
        if token == "--" {
            return &tokens[idx + 1..];
        }
        if token.contains('=') && !token.starts_with('-') {
            idx += 1;
            continue;
        }
        if !token.starts_with('-') {
            break;
        }

        let flag = token.split('=').next().unwrap_or(token);
        let takes_value = value_flags.contains(&flag);
        idx += 1;
        if takes_value && !token.contains('=') && idx < tokens.len() {
            idx += 1;
        }
    }
    &tokens[idx..]
}
fn main() {
    let tokens: Vec<String> = vec!["--chdir=infra".into(), "apply".into()];
    println!(
        "{:?}",
        strip_leading_tokens(&tokens, &["-chdir", "--chdir"])
    );
}
