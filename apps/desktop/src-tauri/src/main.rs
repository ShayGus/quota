//! Desktop entrypoint. Composition and lifecycle live in the library.

#![forbid(unsafe_code)]

fn main() {
    if let Err(error) = quota_desktop_lib::run() {
        eprintln!("quota failed to start: {error}");
        std::process::exit(1);
    }
}
