use serenity::prelude::TypeMapKey;

pub struct HttpClientInstance;

impl TypeMapKey for HttpClientInstance {
    type Value = reqwest::Client;
}
