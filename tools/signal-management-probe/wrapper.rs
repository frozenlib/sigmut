fn main() {
    let script = std::env::current_exe()
        .unwrap()
        .with_file_name("wrapper.py");
    let status = std::process::Command::new(std::env::var_os("PROBE_PYTHON").unwrap())
        .arg(script)
        .args(std::env::args_os().skip(1))
        .status()
        .unwrap();
    std::process::exit(status.code().unwrap_or(1));
}
