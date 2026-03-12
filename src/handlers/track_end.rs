use serenity::{
    all::{CreateActionRow, EditMessage},
    async_trait,
    http::Http,
    model::id::GuildId,
    prelude::{Mutex, RwLock, TypeMap},
};
use songbird::{Call, Event, EventContext, EventHandler, tracks::TrackHandle};
use std::sync::Arc;

use crate::{
    commands::{
        play::{Mode, normal_query_type_resolver},
        queue::{
            build_single_nav_btn, calculate_num_pages, create_queue_embed, forget_queue_message,
        },
        voteskip::forget_skip_votes,
    },
    guild::{
        cache::GuildCacheMap, metadata_store::MetadataStore, settings::GuildSettingsMap,
        stored_queue::GuildStoredQueueMap,
    },
};

pub struct TrackEndHandler {
    pub http: Arc<Http>,
    pub guild_id: GuildId,
    pub call: Arc<Mutex<Call>>,
    pub ctx_data: Arc<RwLock<TypeMap>>,
}

pub struct ModifyQueueHandler {
    pub http: Arc<Http>,
    pub ctx_data: Arc<RwLock<TypeMap>>,
    pub call: Arc<Mutex<Call>>,
    pub guild_id: GuildId,
}

#[async_trait]
impl EventHandler for TrackEndHandler {
    async fn act(&self, _ctx: &EventContext<'_>) -> Option<Event> {
        let (autopause, queue_loop, guild_stored_queue) = {
            let data_rlock = self.ctx_data.read().await;
            let guild_setting = data_rlock
                .get::<GuildSettingsMap>()?
                .get(&self.guild_id)
                .unwrap();
            let guild_stored_queue = data_rlock
                .get::<GuildStoredQueueMap>()?
                .get(&self.guild_id)?
                .clone();

            (
                guild_setting.autopause,
                guild_setting.queue_loop,
                guild_stored_queue,
            )
        };

        if autopause {
            let handler = self.call.lock().await;
            let local_queue = handler.queue();
            local_queue.pause().ok();
        }

        if queue_loop && guild_stored_queue.continue_play {
            let is_queue_empty = {
                let handler = self.call.lock().await;
                handler.queue().is_empty()
            };

            if is_queue_empty {
                for item in guild_stored_queue.queue {
                    if let Err(err) = normal_query_type_resolver(
                        &self.call,
                        &self.http,
                        &self.ctx_data,
                        self.guild_id,
                        &item,
                        Mode::End,
                    )
                    .await
                    {
                        println!("{}", err);
                    }
                }
            }
        } else {
            let current_track = {
                let handler = self.call.lock().await;
                handler.queue().current()
            };

            if let Some(track) = current_track {
                let mut data = self.ctx_data.write().await;
                let metadata_store = data.get_mut::<MetadataStore>().unwrap();
                metadata_store.remove_metadata(&track.uuid().to_string());
            }
        }

        forget_skip_votes(&self.ctx_data, self.guild_id).await.ok();

        None
    }
}

#[async_trait]
impl EventHandler for ModifyQueueHandler {
    async fn act(&self, _ctx: &EventContext<'_>) -> Option<Event> {
        let handler = self.call.lock().await;
        let queue = handler.queue().current_queue();
        drop(handler);

        update_queue_messages(&self.http, &self.ctx_data, &queue, self.guild_id).await;
        None
    }
}

pub async fn update_queue_messages(
    http: &Arc<Http>,
    ctx_data: &Arc<RwLock<TypeMap>>,
    tracks: &[TrackHandle],
    guild_id: GuildId,
) {
    let data = ctx_data.read().await;
    let cache_map = data.get::<GuildCacheMap>().unwrap();

    let mut messages = match cache_map.get(&guild_id) {
        Some(cache) => cache.queue_messages.clone(),
        None => return,
    };
    drop(data);

    for (message, page_lock) in messages.iter_mut() {
        // has the page size shrunk?
        let num_pages = calculate_num_pages(tracks);
        let mut page = page_lock.write().await;
        *page = usize::min(*page, num_pages - 1);

        let embed = create_queue_embed(tracks, *page, ctx_data).await;

        if let Ok(embed) = embed {
            let edit_message = message
                .edit(
                    &http,
                    build_nav_btns(EditMessage::new().add_embed(embed), *page, num_pages),
                )
                .await;

            if edit_message.is_err() {
                forget_queue_message(ctx_data, message, guild_id).await.ok();
            };
        }
    }
}

pub fn build_nav_btns(message: EditMessage, page: usize, num_pages: usize) -> EditMessage {
    let (cant_left, cant_right) = (page < 1, page >= num_pages - 1);

    let components = vec![CreateActionRow::Buttons(vec![
        build_single_nav_btn("<<", cant_left),
        build_single_nav_btn("<", cant_left),
        build_single_nav_btn(">", cant_right),
        build_single_nav_btn(">>", cant_right),
    ])];

    message.components(components)
}
