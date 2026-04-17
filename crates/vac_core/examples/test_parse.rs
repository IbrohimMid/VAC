fn main() {
    let s = r#"g\i\t m\e\r\g\e main"#;
    println!("{:?}", shell_words::split(s));
    let s = r#""g"i"t" 'm'e'r'g'e' main"#;
    println!("{:?}", shell_words::split(s));
}
