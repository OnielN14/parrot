use crate::{
    errors::{ParrotError, verify},
    guild::stored_queue::GuildStoredQueueMap,
    handlers::track_end::update_queue_messages,
    messaging::{message::ParrotMessage, messages::REMOVED_QUEUE},
    utils::{create_embed_response, create_response, get_track_metadata},
};
use serenity::{all::CommandInteraction, builder::CreateEmbed, client::Context, prelude::TypeMap};
use songbird::tracks::TrackHandle;
use std::{cmp::min, convert::TryInto, sync::Arc};
use tokio::sync::RwLock;

pub async fn remove(
    ctx: &Context,
    interaction: &mut CommandInteraction,
) -> Result<(), ParrotError> {
    let guild_id = interaction.guild_id.unwrap();
    let manager = songbird::get(ctx).await.unwrap();
    let call = manager.get(guild_id).unwrap();

    let args = interaction.data.options.clone();

    let remove_index = args.first().unwrap().value.as_i64().unwrap() as isize;

    let remove_until = match args.get(1) {
        Some(arg) => arg.value.as_i64().unwrap() as isize,
        None => remove_index,
    };

    let handler = call.lock().await;
    let queue = handler.queue().current_queue();

    let queue_len: isize = queue.len().try_into().unwrap();
    let remove_until = min(remove_until, queue_len.saturating_sub(1));

    verify(queue_len > 1, ParrotError::QueueEmpty)?;
    verify(
        remove_index < queue_len,
        ParrotError::NotInRange("index", remove_index, 1, queue_len as isize),
    )?;
    verify(
        remove_until >= remove_index,
        ParrotError::NotInRange(
            "until",
            remove_until as isize,
            remove_index,
            queue_len as isize,
        ),
    )?;

    let track = queue.get(remove_index as usize).unwrap();
    let mut data = ctx.data.write().await;
    let guild_stored_queue = data
        .get_mut::<GuildStoredQueueMap>()
        .unwrap()
        .get_mut(&guild_id)
        .unwrap();

    handler.queue().modify_queue(|v| {
        v.drain((remove_index as usize)..=(remove_until as usize));
        guild_stored_queue
            .queue
            .drain((remove_index as usize)..=(remove_until as usize));
    });

    // refetch the queue after modification
    let queue = handler.queue().current_queue();
    drop(handler);
    drop(data);

    if remove_until == remove_index {
        let embed = create_remove_enqueued_embed(track, &ctx.data).await?;
        create_embed_response(&ctx.http, interaction, embed).await?;
    } else {
        create_response(&ctx.http, interaction, ParrotMessage::RemoveMultiple).await?;
    }

    update_queue_messages(&ctx.http, &ctx.data, &queue, guild_id).await;
    Ok(())
}

async fn create_remove_enqueued_embed(
    track: &TrackHandle,
    data: &Arc<RwLock<TypeMap>>,
) -> Result<CreateEmbed, ParrotError> {
    let embed = CreateEmbed::default();
    let metadata = get_track_metadata(track, data).await?;

    Ok(embed
        .field(
            REMOVED_QUEUE,
            format!(
                "[**{}**]({})",
                metadata.title.unwrap(),
                metadata.source_url.unwrap()
            ),
            false,
        )
        .thumbnail(metadata.thumbnail.unwrap()))
}
