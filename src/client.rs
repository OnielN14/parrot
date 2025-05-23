use reqwest;
use serenity::model::gateway::GatewayIntents;
use songbird::serenity::SerenityInit;

use std::{collections::HashMap, env, error::Error};

use crate::{
    guild::{
        cache::GuildCacheMap, http_client::HttpClientInstance, settings::GuildSettingsMap,
        stored_queue::GuildStoredQueueMap,
    },
    handlers::SerenityHandler,
};

pub struct Client {
    client: serenity::Client,
}

impl Client {
    pub async fn default() -> Result<Client, Box<dyn Error>> {
        let token = env::var("DISCORD_TOKEN").expect("Fatality! DISCORD_TOKEN not set!");
        Client::new(token).await
    }

    pub async fn new(token: String) -> Result<Client, Box<dyn Error>> {
        let application_id = env::var("DISCORD_APP_ID")
            .expect("Fatality! DISCORD_APP_ID not set!")
            .parse()?;

        let gateway_intents = GatewayIntents::non_privileged();

        let client = serenity::Client::builder(token, gateway_intents)
            .event_handler(SerenityHandler)
            .application_id(application_id)
            .register_songbird()
            .await?;

        let mut data = client.data.write().await;
        data.insert::<GuildCacheMap>(HashMap::default());
        data.insert::<GuildSettingsMap>(HashMap::default());
        data.insert::<GuildStoredQueueMap>(HashMap::default());
        data.insert::<HttpClientInstance>(reqwest::Client::new());

        drop(data);

        Ok(Client { client })
    }

    pub async fn start(&mut self) -> Result<(), serenity::Error> {
        self.client.start().await
    }
}
