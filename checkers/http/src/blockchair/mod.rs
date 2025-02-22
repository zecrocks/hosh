pub mod blockchairdotcom;
pub mod blockchairdotonion;

pub use blockchairdotcom::get_blockchain_info;
pub use blockchairdotonion::get_blockchain_info as get_onion_blockchain_info;