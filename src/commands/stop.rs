use crate::{
    errors::{ParrotError, verify},
    guild::stored_queue::GuildStoredQueueMap,
    handlers::track_end::update_queue_messages,
    messaging::message::ParrotMessage,
    utils::create_response,
};
use serenity::{all::CommandInteraction, client::Context};

pub async fn stop(ctx: &Context, interaction: &mut CommandInteraction) -> Result<(), ParrotError> {
    let guild_id = interaction.guild_id.unwrap();
    let manager = songbird::get(ctx).await.unwrap();
    let call = manager.get(guild_id).unwrap();

    let mut data = ctx.data.write().await;
    let stored_queue = data.get_mut::<GuildStoredQueueMap>().unwrap();
    let guild_stored_queue = stored_queue.get_mut(&guild_id).unwrap();
    guild_stored_queue.continue_play = false;
    guild_stored_queue.queue.clear();
    drop(data);

    let handler = call.lock().await;
    let queue = handler.queue();

    verify(!queue.is_empty(), ParrotError::NothingPlaying)?;
    queue.stop();

    // refetch the queue after modification
    let queue = handler.queue().current_queue();
    drop(handler);

    create_response(&ctx.http, interaction, ParrotMessage::Stop).await?;
    update_queue_messages(&ctx.http, &ctx.data, &queue, guild_id).await;
    Ok(())
}
