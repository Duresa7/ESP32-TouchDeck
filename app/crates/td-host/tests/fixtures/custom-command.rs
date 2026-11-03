use std::io::Write;
fn main() {
    let args: Vec<_> = std::env::args().collect();
    match args[1].as_str() {
        "slow" => {
            std::thread::sleep(std::time::Duration::from_secs(2));
            std::fs::write(&args[2], "ran").unwrap();
        }
        "large" => print!("{}", "x".repeat(65537)),
        "literal" => print!("{}", args[2]),
        "marker" => {
            std::fs::write(&args[2], "ran").unwrap();
            print!("42");
        }
        "fork" => {
            std::process::Command::new(std::env::current_exe().unwrap())
                .arg("late")
                .args(&args[2..])
                .spawn()
                .unwrap();
        }
        "late" => {
            let mut probe = std::net::TcpStream::connect(&args[3]).unwrap();
            probe.write_all(b"started").unwrap();
            std::thread::sleep(std::time::Duration::from_secs(60));
            std::fs::write(&args[2], "ran").unwrap();
            drop(probe);
        }
        "endless" => {
            let mut out = std::io::stdout();
            let mut chunks = 0usize;
            while out.write_all(&[b'x'; 8192]).is_ok() {
                chunks += 1;
                if chunks == 128 {
                    if let Some(marker) = args.get(2) {
                        std::fs::write(marker, "unbounded").unwrap();
                    }
                }
            }
        }
        _ => print!("42"),
    }
}
