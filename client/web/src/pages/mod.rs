//! One module per route. Each is owned by its file — do not move components
//! between files (the router in `main.rs` imports them by path).

pub mod admin_discounts;
pub mod admin_generator;
pub mod admin_index;
pub mod admin_users;
pub mod game_completed;
pub mod game_new;
pub mod game_play;
pub mod games;
pub mod home;
pub mod login;
pub mod profile;
pub mod signup;
pub mod stats;
pub mod verify_email;
