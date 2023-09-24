use std::net::SocketAddr;

use clap::Parser;

#[derive(Parser, Debug)]
#[clap(
    name = "made_in_heaven",
    rename_all = "kebab-case",
    rename_all_env = "screaming-snake"
)]
pub struct Args {
    #[clap(default_value = "0.0.0.0:2053", env)]
    pub host: SocketAddr,
}
