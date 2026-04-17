fn main() {
    let s = r#"echo $(git merge main)"#;
    println!("{:?}", shell_words::split(s));
    let s = r#"echo `git merge main`"#;
    println!("{:?}", shell_words::split(s));
    let s = r#"git merge \
    main"#;
    println!("{:?}", shell_words::split(s));
}
