use std::process::ExitCode;

mod app;
mod cli;
mod discord_retry;
mod output;
mod report;
mod safety;
mod service_guard;

fn main() -> ExitCode {
    match std::thread::Builder::new()
        .name("xiii-superbot-cli".to_owned())
        .stack_size(16 * 1024 * 1024)
        .spawn(app::run)
    {
        Ok(handle) => match handle.join() {
            Ok(code) => code,
            Err(_) => {
                eprintln!("[FAIL] xiii-superbot CLI thread panicked");
                ExitCode::from(2)
            }
        },
        Err(err) => {
            eprintln!("[FAIL] failed to start xiii-superbot CLI thread: {err}");
            ExitCode::from(2)
        }
    }
}
