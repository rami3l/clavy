use clap::Parser;
use clavy::error::Result;
use embed_plist::embed_info_plist;

use crate::cmd::Clavy;

mod cmd;

mod _built {
    include!(concat!(env!("OUT_DIR"), "/built.rs"));
}

embed_info_plist!("../assets/Info.plist");

fn main() -> Result<()> {
    Clavy::parse().dispatch()
}
