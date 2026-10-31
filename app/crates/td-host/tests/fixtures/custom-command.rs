fn main() {
    let args: Vec<_> = std::env::args().collect();
    match args[1].as_str() {
        "slow" => { std::thread::sleep(std::time::Duration::from_secs(2)); std::fs::write(&args[2], "ran").unwrap(); }
        "large" => print!("{}", "x".repeat(65537)),
        "literal" => print!("{}", args[2]),
        "marker" => { std::fs::write(&args[2], "ran").unwrap(); print!("42"); }
        _ => print!("42"),
    }
}
