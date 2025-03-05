pub mod blockchairdotcom;
pub mod blockchairdotonion;

// Export onion source
pub use blockchairdotonion::get_blockchain_info as get_onion_blockchain_info;

// Maintain backward compatibility
pub use blockchairdotcom::get_blockchain_info;