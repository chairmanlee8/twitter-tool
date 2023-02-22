use crate::twitter_client::api;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct UserConfig {
    pub starred_accounts: HashMap<String, api::User>,
}

impl UserConfig {
    pub fn is_starred(&self, user_id: &str) -> bool {
        self.starred_accounts.contains_key(user_id)
    }

    pub fn star_account(&mut self, user: &api::User) {
        self.starred_accounts
            .insert(user.id.to_string(), user.clone());
    }

    pub fn unstar_account(&mut self, user: &api::User) {
        self.starred_accounts.remove(&user.id.to_string());
    }
}
