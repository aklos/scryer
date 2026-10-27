//! Agent integration as data: the entries scryer writes into each agent tool's
//! config, and the developer's preferences for which agent to launch. [domain]

pub mod agent_entries;
pub mod agent_settings;

pub use agent_entries::*;
pub use agent_settings::*;
