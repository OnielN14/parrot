use crate::{
    errors::{verify, ParrotError},
    messaging::message::ParrotMessage,
    utils::{create_response, AuxMetadataTypeMapKey},
};
use serenity::{all::CommandInteraction, client::Context};
use songbird::{tracks::TrackHandle, Call};
use std::cmp::min;
use tokio::sync::MutexGuard;

pub async fn skip(ctx: &Context, interaction: &mut CommandInteraction) -> Result<(), ParrotError> {
    let guild_id = interaction.guild_id.unwrap();
    let manager = songbird::get(ctx).await.unwrap();
    let call = manager.get(guild_id).unwrap();

    let args = interaction.data.options.clone();
    let to_skip = match args.first() {
        Some(arg) => arg.value.as_i64().unwrap() as usize,
        None => 1,
    };

    let handler = call.lock().await;
    let queue = handler.queue();

    verify(!queue.is_empty(), ParrotError::NothingPlaying)?;

    let tracks_to_skip = min(to_skip, queue.len());

    handler.queue().modify_queue(|v| {
        v.drain(1..tracks_to_skip);
    });

    force_skip_top_track(&handler).await?;
    create_skip_response(ctx, interaction, &handler, tracks_to_skip).await
}

pub async fn create_skip_response(
    ctx: &Context,
    interaction: &mut CommandInteraction,
    handler: &MutexGuard<'_, Call>,
    tracks_to_skip: usize,
) -> Result<(), ParrotError> {
    match handler.queue().current() {
        Some(track) => {
            let track_typemap_read_lock = track.typemap().read().await;
            let metadata = track_typemap_read_lock
                .get::<AuxMetadataTypeMapKey>()
                .unwrap()
                .clone();

            create_response(
                &ctx.http,
                interaction,
                ParrotMessage::SkipTo {
                    title: metadata.title.unwrap(),
                    url: metadata.source_url.unwrap(),
                },
            )
            .await
        }
        None => {
            if tracks_to_skip > 1 {
                create_response(&ctx.http, interaction, ParrotMessage::SkipAll).await
            } else {
                create_response(&ctx.http, interaction, ParrotMessage::Skip).await
            }
        }
    }
}

pub async fn force_skip_top_track(
    handler: &MutexGuard<'_, Call>,
) -> Result<Vec<TrackHandle>, ParrotError> {
    // this is an odd sequence of commands to ensure the queue is properly updated
    // apparently, skipping/stopping a track takes a while to remove it from the queue
    // also, manually removing tracks doesn't trigger the next track to play
    // so first, stop the top song, manually remove it and then resume playback
    handler.queue().current().unwrap().stop().ok();
    let _ = handler.queue().dequeue(0);
    handler.queue().resume().ok();

    Ok(handler.queue().current_queue())
}
