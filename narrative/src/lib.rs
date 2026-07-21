pub mod objectives;

pub mod prelude {
    pub use super::objectives::{
        components::*,
        events::*,
        registry::*,
        relations::*,
    };
}
