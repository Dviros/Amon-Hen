fn main() {
    let code = match std::panic::catch_unwind(amon_hen::run_from_env) {
        Ok(code) => code,
        Err(payload) => {
            let message = if let Some(message) = payload.downcast_ref::<&str>() {
                (*message).to_string()
            } else if let Some(message) = payload.downcast_ref::<String>() {
                message.clone()
            } else {
                "panic with non-string payload".to_string()
            };
            eprintln!("Amon Hen crashed: {message}");
            if let Some(dir) = std::env::var_os("AMON_HEN_RUN_DIR") {
                let dir = std::path::PathBuf::from(dir);
                let _ = std::fs::create_dir_all(&dir);
                let _ = std::fs::write(
                    dir.join("last-error.txt"),
                    format!("Amon Hen crashed: {message}\n"),
                );
                eprintln!(
                    "Amon Hen crash artifact: {}",
                    dir.join("last-error.txt").display()
                );
            }
            101
        }
    };
    std::process::exit(code);
}
