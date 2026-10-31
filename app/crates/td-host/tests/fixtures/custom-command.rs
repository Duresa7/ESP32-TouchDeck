use std::io::Write;
fn main() {
    let args: Vec<_> = std::env::args().collect();
    match args[1].as_str() {
        "slow" => { std::thread::sleep(std::time::Duration::from_secs(2)); std::fs::write(&args[2], "ran").unwrap(); }
        "large" => print!("{}", "x".repeat(65537)),
        "literal" => print!("{}", args[2]),
        "marker" => { std::fs::write(&args[2], "ran").unwrap(); print!("42"); }
        "fork" => { std::process::Command::new(std::env::current_exe().unwrap()).arg("late").arg(&args[2]).spawn().unwrap(); }
        "late" => { std::thread::sleep(std::time::Duration::from_secs(3)); std::fs::write(&args[2], "ran").unwrap(); }
        "endless" => { let mut out = std::io::stdout(); while out.write_all(&[b'x'; 8192]).is_ok() {} }
        _ => print!("42"),
    }
}
