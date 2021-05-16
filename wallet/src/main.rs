use std::str::FromStr;

use argh::FromArgs;

use bitcoin::Address;

use nakamoto_common::block::Height;
use nakamoto_wallet::logger;

/// A Bitcoin wallet.
#[derive(FromArgs)]
pub struct Options {
    /// connect to the specified peer
    #[argh(option)]
    pub connect: String,
    /// watch the following addresses
    #[argh(option)]
    pub addresses: Vec<Address>,
    /// wallet genesis height, from which to start scanning
    #[argh(option)]
    pub genesis: Height,
    /// enable debug logging
    #[argh(switch)]
    pub debug: bool,
}

impl Options {
    pub fn from_env() -> Self {
        argh::from_env()
    }
}

fn main() {
    //     let flags = ServiceFlags::from(1149);

    //     println!("{}", flags.has(ServiceFlags::BLOOM));
    //     println!("{}", flags.has(ServiceFlags::COMPACT_FILTERS));
    // return;
    logger::init(log::Level::Debug).expect("initializing logger for the first time");

    if let Err(err) = nakamoto_wallet::run(
        [Address::from_str("mkHS9ne12qx9pS9VojpwU5xtRd4T7X7ZUt").unwrap()].into(),
        0,
    ) {
        log::error!("Fatal: {}", err);
        std::process::exit(1);
    }
}
