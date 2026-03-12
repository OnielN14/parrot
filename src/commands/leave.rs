use crate::{
    errors::ParrotError, guild::stored_queue::GuildStoredQueueMap,
    messaging::message::ParrotMessage, utils::create_response,
};
use serenity::{all::CommandInteraction, client::Context};

pub async fn leave(ctx: &Context, interaction: &mut CommandInteraction) -> Result<(), ParrotError> {
    let guild_id = interaction.guild_id.unwrap();
    let manager = songbird::get(ctx).await.unwrap();
    let mut data = ctx.data.write().await;

    if let Some(stored_queue) = data.get_mut::<GuildStoredQueueMap>()
        && let Some(guild_stored_queue) = stored_queue.get_mut(&guild_id)
    {
        guild_stored_queue.queue.clear();
    }

    drop(data);

    let call = manager.get(guild_id).unwrap();
    let mut handler = call.lock().await;
    handler.remove_all_global_events();
    drop(handler);

    manager.remove(guild_id).await.unwrap();

    create_response(&ctx.http, interaction, ParrotMessage::Leaving).await
}
