pub mod proto {
    include!(concat!(env!("OUT_DIR"), "/eadp.rtm.rs"));
}

pub mod client;
pub mod connection;
