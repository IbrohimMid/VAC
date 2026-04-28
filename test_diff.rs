use std::process::Command;

fn main() {
    let output = Command::new("git").arg("status").arg("--porcelain").output().unwrap();
    let paths = String::from_utf8_lossy(&output.stdout);
    for line in paths.lines() {
        if line.len() > 3 {
            let path = &line[3..];
            println!("{}", path);
        }
    }
}
