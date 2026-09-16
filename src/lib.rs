//! BrighTO-Router — thư viện lõi. main.rs chỉ là vỏ; mọi logic nằm ở đây để test được.
//! Module tree: contract (kiểu + state), auth, budget, route, proxy, ledger, admin, metrics, handlers.

pub mod admin;
pub mod auth;
pub mod budget;
pub mod config;
pub mod contract;
pub mod handlers;
pub mod ledger;
pub mod metrics;
pub mod proxy;
pub mod route;
