// Both development builds and installed builds are desktop applications.
#![cfg_attr(windows, windows_subsystem = "windows")]

fn main() {
    let mut args = std::env::args().skip(1);
    if args.next().as_deref() == Some("credential") {
        let Some(provider_id) = args.next() else {
            eprintln!("DeepPi credential helper requires a provider id");
            std::process::exit(2);
        };
        if args.next().is_some() {
            eprintln!("DeepPi credential helper received unexpected arguments");
            std::process::exit(2);
        }
        if let Err(error) = deeppi_lib::credential_helper(&provider_id) {
            eprintln!("DeepPi credential helper failed: {error}");
            std::process::exit(1);
        }
        return;
    }
    deeppi_lib::run()
}
