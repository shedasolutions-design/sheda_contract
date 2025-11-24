// Find all our documentation at https://docs.near.org
pub mod internal;
pub mod views;
pub mod events;
pub mod admin;
pub mod models;
use near_sdk::{log, near};

// Define the contract structure
#[near(contract_state)]
pub struct ShedaContract {
    
}

// Define the default, which automatically initializes the contract
impl Default for ShedaContract {
    fn default() -> Self {
        Self {
            
        }
    }
}

// Implement the contract structure
#[near]
impl ShedaContract {
    // Public method - returns the greeting saved, defaulting to DEFAULT_GREETING
    
}

/*
 * The rest of this file holds the inline tests for the code above
 * Learn more about Rust tests: https://doc.rust-lang.org/book/ch11-01-writing-tests.html
 */
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn get_default_greeting() {
        todo!()
    }

    #[test]
    fn set_then_get_greeting() {
        todo!()
    }
}
