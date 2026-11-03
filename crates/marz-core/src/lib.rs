//! Business logic shared by the Linux and Windows apps: the App API client, session rotation, encrypted
//! storage and the payment rules. Ported from Android RC6 (com.marzvpn.app.api), behaviour kept.
pub mod api;
pub mod checkout;
pub mod http;
pub mod model;
pub mod refresh;
pub mod tls;
pub mod vault;
pub mod controller;
pub mod helper_client;
pub mod prefs;
pub mod tunnel_check;
pub mod links;
