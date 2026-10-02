//! `pgrust-init DATADIR SHAREDIR`: a data directory and a share directory
//! without PostgreSQL installed (the `init` feature; see
//! `main_main::embedded`). Each target must be absent or an empty directory.
//! Then: `PGRUST_PGSHAREDIR=SHAREDIR PGRUST_TZDIR=SHAREDIR/timezone postgres -D DATADIR`.

use std::path::Path;
use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [data, share] = args.as_slice() else {
        eprintln!("usage: pgrust-init DATADIR SHAREDIR");
        return ExitCode::from(2);
    };
    if let Err(e) = main_main::embedded::init_data_dir(Path::new(data)) {
        eprintln!("pgrust-init: data directory: {e}");
        return ExitCode::FAILURE;
    }
    if let Err(e) = main_main::embedded::install_share_dir(Path::new(share)) {
        eprintln!("pgrust-init: share directory: {e}");
        return ExitCode::FAILURE;
    }
    println!(
        "pgrust-init: {data} (cluster made by {}) and {share}",
        main_main::embedded::pristine_postgres_version()
    );
    ExitCode::SUCCESS
}
