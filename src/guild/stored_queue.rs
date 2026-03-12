use std::collections::HashMap;

use serenity::{model::id::GuildId, prelude::TypeMapKey};

use crate::commands::play::QueryType;

#[derive(Default, Debug, Clone)]
pub struct GuildStoredQueue {
    pub continue_play: bool,
    pub queue: Vec<QueryType>,
}

impl GuildStoredQueue {
    pub fn new() -> Self {
        Self {
            queue: Vec::new(),
            continue_play: true,
        }
    }
}

pub struct GuildStoredQueueMap;

impl TypeMapKey for GuildStoredQueueMap {
    type Value = HashMap<GuildId, GuildStoredQueue>;
}
