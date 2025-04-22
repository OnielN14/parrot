use crate::{
    commands::{skip::force_skip_top_track, summon::summon},
    errors::{verify, ParrotError},
    guild::{
        http_client::HttpClientInstance,
        settings::{GuildSettings, GuildSettingsMap},
        stored_queue::{GuildStoredQueue, GuildStoredQueueMap},
    },
    handlers::track_end::update_queue_messages,
    messaging::{
        message::ParrotMessage,
        messages::{PLAY_QUEUE, PLAY_TOP, SPOTIFY_AUTH_FAILED, TRACK_DURATION, TRACK_TIME_TO_PLAY},
    },
    sources::spotify::{Spotify, SPOTIFY},
    utils::{
        compare_domains, create_now_playing_embed, create_response, edit_embed_response,
        edit_response, get_human_readable_timestamp, AuxMetadataTypeMapKey,
    },
};
use serenity::{
    all::{CommandInteraction, CreateEmbedFooter},
    builder::CreateEmbed,
    client::Context,
    futures::executor::block_on,
    http::Http,
    model::id::GuildId,
    prelude::Mutex,
};
use songbird::{
    input::{Compose, YoutubeDl},
    tracks::TrackHandle,
    typemap::TypeMap,
    Call,
};
use std::{cmp::Ordering, error::Error as StdError, sync::Arc, time::Duration};
use tokio::sync::RwLock;
use url::Url;

use reqwest;

#[derive(Clone, Copy, Debug)]
pub enum Mode {
    End,
    Next,
    All,
    Reverse,
    Shuffle,
    Jump,
}

#[derive(Clone, Debug)]
pub enum QueryType {
    Keywords(String),
    KeywordList(Vec<String>),
    VideoLink(String),
    PlaylistLink(String),
}

pub async fn play(ctx: &Context, interaction: &mut CommandInteraction) -> Result<(), ParrotError> {
    let args = interaction.data.options.clone();
    let first_arg = args.first().unwrap();

    let mode = match first_arg.name.as_str() {
        "next" => Mode::Next,
        "all" => Mode::All,
        "reverse" => Mode::Reverse,
        "shuffle" => Mode::Shuffle,
        "jump" => Mode::Jump,
        _ => Mode::End,
    };

    let url = match mode {
        Mode::End => first_arg.value.as_str().unwrap(),
        _ => first_arg.value.as_str().unwrap(),
    };

    let guild_id = interaction.guild_id.unwrap();
    let manager = songbird::get(ctx).await.unwrap();

    // try to join a voice channel if not in one just yet
    summon(ctx, interaction, false).await?;
    let call = manager.get(guild_id).unwrap();

    // determine whether this is a link or a query string
    let query_type = match Url::parse(url) {
        Ok(url_data) => match url_data.host_str() {
            Some("open.spotify.com") => {
                let spotify = SPOTIFY.lock().await;
                let spotify = verify(spotify.as_ref(), ParrotError::Other(SPOTIFY_AUTH_FAILED))?;
                Some(Spotify::extract(spotify, url).await?)
            }
            Some(other) => {
                let mut data = ctx.data.write().await;
                let settings = data.get_mut::<GuildSettingsMap>().unwrap();
                let guild_settings = settings
                    .entry(guild_id)
                    .or_insert_with(|| GuildSettings::new(guild_id));

                let is_allowed = guild_settings
                    .allowed_domains
                    .iter()
                    .any(|d| compare_domains(d, other));

                let is_banned = guild_settings
                    .banned_domains
                    .iter()
                    .any(|d| compare_domains(d, other));

                if is_banned || (guild_settings.banned_domains.is_empty() && !is_allowed) {
                    return create_response(
                        &ctx.http,
                        interaction,
                        ParrotMessage::PlayDomainBanned {
                            domain: other.to_string(),
                        },
                    )
                    .await;
                }

                if url.contains("list=") {
                    Some(QueryType::PlaylistLink(url.to_string()))
                } else {
                    Some(QueryType::VideoLink(url.to_string()))
                }
            }
            None => None,
        },
        Err(_) => {
            let mut data = ctx.data.write().await;
            let settings = data.get_mut::<GuildSettingsMap>().unwrap();
            let guild_settings = settings
                .entry(guild_id)
                .or_insert_with(|| GuildSettings::new(guild_id));

            if guild_settings.banned_domains.contains("youtube.com")
                || (guild_settings.banned_domains.is_empty()
                    && !guild_settings.allowed_domains.contains("youtube.com"))
            {
                return create_response(
                    &ctx.http,
                    interaction,
                    ParrotMessage::PlayDomainBanned {
                        domain: "youtube.com".to_string(),
                    },
                )
                .await;
            }

            Some(QueryType::Keywords(url.to_string()))
        }
    };

    let query_type = verify(
        query_type,
        ParrotError::Other("Something went wrong while parsing your query!"),
    )?;

    let mut data = ctx.data.write().await;
    let stored_queue_map = data.get_mut::<GuildStoredQueueMap>().unwrap();
    let guild_stored_queue = stored_queue_map
        .entry(guild_id)
        .or_insert_with(GuildStoredQueue::new);

    guild_stored_queue.queue.push(query_type.clone());
    guild_stored_queue.continue_play = true;

    drop(data);

    let data = ctx.data.read().await;
    let http_client = data.get::<HttpClientInstance>().unwrap();

    // reply with a temporary message while we fetch the source
    // needed because interactions must be replied within 3s and queueing takes longer
    create_response(&ctx.http, interaction, ParrotMessage::Search).await?;

    let handler = call.lock().await;
    let queue_was_empty = handler.queue().is_empty();
    drop(handler);

    match mode {
        Mode::End => {
            normal_query_type_resolver(&call, &ctx.http, &ctx.data, guild_id, &query_type, mode)
                .await?
        }
        Mode::Next => match query_type.clone() {
            QueryType::Keywords(_) | QueryType::VideoLink(_) => {
                let queue = insert_track(&call, http_client, &query_type, 1).await?;
                update_queue_messages(&ctx.http, &ctx.data, &queue, guild_id).await;
            }
            QueryType::PlaylistLink(url) => {
                let urls = get_urls_from_playlist(http_client, url, None).await?;

                for (idx, url) in urls.into_iter().flatten().enumerate() {
                    let Ok(queue) =
                        insert_track(&call, http_client, &QueryType::VideoLink(url), idx + 1).await
                    else {
                        continue;
                    };
                    update_queue_messages(&ctx.http, &ctx.data, &queue, guild_id).await;
                }
            }
            QueryType::KeywordList(keywords_list) => {
                for (idx, keywords) in keywords_list.into_iter().enumerate() {
                    let queue =
                        insert_track(&call, http_client, &QueryType::Keywords(keywords), idx + 1)
                            .await?;
                    update_queue_messages(&ctx.http, &ctx.data, &queue, guild_id).await;
                }
            }
        },
        Mode::Jump => match query_type.clone() {
            QueryType::Keywords(_) | QueryType::VideoLink(_) => {
                let mut queue = enqueue_track(&call, http_client, &query_type).await?;

                if !queue_was_empty {
                    rotate_tracks(&call, 1).await.ok();
                    queue = force_skip_top_track(&call.lock().await).await?;
                }

                update_queue_messages(&ctx.http, &ctx.data, &queue, guild_id).await;
            }
            QueryType::PlaylistLink(url) => {
                let urls = get_urls_from_playlist(http_client, url, None).await?;

                let mut insert_idx = 1;

                for (i, url) in urls.into_iter().flatten().enumerate() {
                    let Ok(mut queue) =
                        insert_track(&call, http_client, &QueryType::VideoLink(url), insert_idx)
                            .await
                    else {
                        continue;
                    };

                    if i == 0 && !queue_was_empty {
                        queue = force_skip_top_track(&call.lock().await).await?;
                    } else {
                        insert_idx += 1;
                    }

                    update_queue_messages(&ctx.http, &ctx.data, &queue, guild_id).await;
                }
            }
            QueryType::KeywordList(keywords_list) => {
                let mut insert_idx = 1;

                for (i, keywords) in keywords_list.into_iter().enumerate() {
                    let mut queue = insert_track(
                        &call,
                        http_client,
                        &QueryType::Keywords(keywords),
                        insert_idx,
                    )
                    .await?;

                    if i == 0 && !queue_was_empty {
                        queue = force_skip_top_track(&call.lock().await).await?;
                    } else {
                        insert_idx += 1;
                    }

                    update_queue_messages(&ctx.http, &ctx.data, &queue, guild_id).await;
                }
            }
        },
        Mode::All | Mode::Reverse | Mode::Shuffle => match query_type.clone() {
            QueryType::VideoLink(url) | QueryType::PlaylistLink(url) => {
                let urls = get_urls_from_playlist(http_client, url, None).await?;

                for url in urls.into_iter().flatten() {
                    let Ok(queue) =
                        enqueue_track(&call, http_client, &QueryType::VideoLink(url)).await
                    else {
                        continue;
                    };
                    update_queue_messages(&ctx.http, &ctx.data, &queue, guild_id).await;
                }
            }
            QueryType::KeywordList(keywords_list) => {
                for keywords in keywords_list.into_iter() {
                    let queue =
                        enqueue_track(&call, http_client, &QueryType::Keywords(keywords)).await?;
                    update_queue_messages(&ctx.http, &ctx.data, &queue, guild_id).await;
                }
            }
            _ => {
                edit_response(&ctx.http, interaction, ParrotMessage::PlayAllFailed).await?;
                return Ok(());
            }
        },
    }

    let handler = call.lock().await;

    // refetch the queue after modification
    let queue = handler.queue().current_queue();
    drop(handler);

    match queue.len().cmp(&1) {
        Ordering::Greater => {
            let estimated_time = calculate_time_until_play(&queue, mode).await.unwrap();

            match (query_type, mode) {
                (QueryType::VideoLink(_) | QueryType::Keywords(_), Mode::Next) => {
                    let track = queue.get(1).unwrap();
                    let embed = create_queued_embed(PLAY_TOP, track, estimated_time).await;

                    edit_embed_response(&ctx.http, interaction, embed).await?;
                }
                (QueryType::VideoLink(_) | QueryType::Keywords(_), Mode::End) => {
                    let track = queue.last().unwrap();
                    let embed = create_queued_embed(PLAY_QUEUE, track, estimated_time).await;

                    edit_embed_response(&ctx.http, interaction, embed).await?;
                }
                (QueryType::PlaylistLink(_) | QueryType::KeywordList(_), _) => {
                    edit_response(&ctx.http, interaction, ParrotMessage::PlaylistQueued).await?;
                }
                (_, _) => {}
            }
        }
        Ordering::Equal => {
            let track = queue.first().unwrap();
            let embed = create_now_playing_embed(track).await;

            edit_embed_response(&ctx.http, interaction, embed).await?;
        }
        _ => println!("Ignore queue reordering"),
    }

    Ok(())
}

async fn calculate_time_until_play(queue: &[TrackHandle], mode: Mode) -> Option<Duration> {
    if queue.is_empty() {
        return None;
    }

    let top_track = queue.first()?;
    let top_track_elapsed = top_track.get_info().await.unwrap().position;
    let top_track_typemap_read_lock = top_track.typemap().read().await;
    let top_track_aux_metadata = top_track_typemap_read_lock
        .get::<AuxMetadataTypeMapKey>()
        .unwrap();

    let top_track_duration = match top_track_aux_metadata.duration {
        Some(duration) => duration,
        None => return Some(Duration::MAX),
    };

    match mode {
        Mode::Next => Some(top_track_duration - top_track_elapsed),
        _ => {
            let center = &queue[1..queue.len() - 1];
            let livestreams = center.len()
                - center
                    .iter()
                    .filter_map(|t| {
                        block_on(async {
                            t.typemap()
                                .read()
                                .await
                                .get::<AuxMetadataTypeMapKey>()
                                .unwrap()
                                .duration
                        })
                    })
                    .count();

            // if any of the tracks before are livestreams, the new track will never play
            if livestreams > 0 {
                return Some(Duration::MAX);
            }

            let durations = center.iter().fold(Duration::ZERO, |acc, x| {
                let duration = block_on(async {
                    x.typemap()
                        .read()
                        .await
                        .get::<AuxMetadataTypeMapKey>()
                        .unwrap()
                        .duration
                });
                acc + duration.unwrap()
            });

            Some(durations + top_track_duration - top_track_elapsed)
        }
    }
}

async fn create_queued_embed(
    title: &str,
    track: &TrackHandle,
    estimated_time: Duration,
) -> CreateEmbed {
    let mut embed = CreateEmbed::default();
    let track_typemap_read_lock = track.typemap().read().await;
    let metadata = track_typemap_read_lock
        .get::<AuxMetadataTypeMapKey>()
        .unwrap()
        .clone();

    embed = embed.thumbnail(metadata.thumbnail.unwrap()).field(
        title,
        format!(
            "[**{}**]({})",
            metadata.title.unwrap(),
            metadata.source_url.unwrap()
        ),
        false,
    );

    let footer_text = format!(
        "{}{}\n{}{}",
        TRACK_DURATION,
        get_human_readable_timestamp(metadata.duration),
        TRACK_TIME_TO_PLAY,
        get_human_readable_timestamp(Some(estimated_time))
    );

    embed.footer(CreateEmbedFooter::new(footer_text))
}

fn get_track_source(http_client: reqwest::Client, query_type: QueryType) -> YoutubeDl {
    match query_type {
        QueryType::VideoLink(url) => YoutubeDl::new(http_client, url),

        QueryType::Keywords(query) => YoutubeDl::new_search(http_client, query),
        _ => unreachable!(),
    }
}

async fn enqueue_track(
    call: &Arc<Mutex<Call>>,
    http_client: &reqwest::Client,
    query_type: &QueryType,
) -> Result<Vec<TrackHandle>, ParrotError> {
    let source = get_track_source(http_client.clone(), query_type.clone());

    let mut source_c = source.clone();
    let mut handler = call.lock().await;
    let track_handler = handler.enqueue_input(source.clone().into()).await;

    let mut track_handle_typemap = track_handler.typemap().write().await;
    let aux_metadata = source_c.aux_metadata().await.map_err(|err| {
        println!("{:?}", err);

        ParrotError::Other("Unable to get AuxMetadata")
    })?;

    track_handle_typemap.insert::<AuxMetadataTypeMapKey>(aux_metadata.clone());

    if let Some(title) = aux_metadata.title {
        println!("[INFO] queueing {}", title);
    }

    Ok(handler.queue().current_queue())
}

async fn insert_track(
    call: &Arc<Mutex<Call>>,
    http_client: &reqwest::Client,
    query_type: &QueryType,
    idx: usize,
) -> Result<Vec<TrackHandle>, ParrotError> {
    let handler = call.lock().await;
    let queue_size = handler.queue().len();
    drop(handler);

    if queue_size <= 1 {
        let queue = enqueue_track(call, http_client, query_type).await?;
        return Ok(queue);
    }

    verify(
        idx > 0 && idx <= queue_size,
        ParrotError::NotInRange("index", idx as isize, 1, queue_size as isize),
    )?;

    enqueue_track(call, http_client, query_type).await?;

    let handler = call.lock().await;
    handler.queue().modify_queue(|queue| {
        let back = queue.pop_back().unwrap();
        queue.insert(idx, back);
    });

    Ok(handler.queue().current_queue())
}

async fn rotate_tracks(
    call: &Arc<Mutex<Call>>,
    n: usize,
) -> Result<Vec<TrackHandle>, Box<dyn StdError>> {
    let handler = call.lock().await;

    verify(
        handler.queue().len() > 2,
        ParrotError::Other("cannot rotate queues smaller than 3 tracks"),
    )?;

    handler.queue().modify_queue(|queue| {
        let mut not_playing = queue.split_off(1);
        not_playing.rotate_right(n);
        queue.append(&mut not_playing);
    });

    Ok(handler.queue().current_queue())
}

pub async fn normal_query_type_resolver(
    call: &Arc<Mutex<Call>>,
    http: &Arc<Http>,
    data: &Arc<RwLock<TypeMap>>,
    guild_id: GuildId,
    query_type: &QueryType,
    mode: Mode,
) -> Result<(), ParrotError> {
    let data_instance = data.read().await;
    let http_client = data_instance.get::<HttpClientInstance>().unwrap();

    match query_type.clone() {
        QueryType::Keywords(_) | QueryType::VideoLink(_) => {
            let queue = enqueue_track(call, http_client, query_type).await?;
            update_queue_messages(http, data, &queue, guild_id).await;
            Ok(())
        }
        QueryType::PlaylistLink(url) => {
            let urls = get_urls_from_playlist(http_client, url, Some(mode)).await?;

            for url in urls.iter().filter_map(|v| v.clone()) {
                let Ok(queue) =
                    enqueue_track(call, http_client, &QueryType::VideoLink(url.to_string())).await
                else {
                    continue;
                };
                update_queue_messages(http, data, &queue, guild_id).await;
            }
            Ok(())
        }
        QueryType::KeywordList(keywords_list) => {
            for keywords in keywords_list.iter() {
                let queue = enqueue_track(
                    call,
                    http_client,
                    &QueryType::Keywords(keywords.to_string()),
                )
                .await?;
                update_queue_messages(http, data, &queue, guild_id).await;
            }
            Ok(())
        }
    }
}

pub async fn get_urls_from_playlist(
    http_client: &reqwest::Client,
    url: String,
    mode: Option<Mode>,
) -> Result<Vec<Option<String>>, ParrotError> {
    let ytdl = YoutubeDl::new(http_client.clone(), url);
    let mut args = vec!["--flat-playlist", "-j"];

    if let Some(mode) = mode {
        match mode {
            Mode::Reverse => args.push("--playlist-reverse"),
            Mode::Shuffle => args.push("--playlist-random"),
            _ => {}
        }
    }

    let mut ytdl = ytdl.user_args(args.iter().map(|v| (*v).to_owned()).collect());

    let result = ytdl
        .search(None)
        .await
        .map_err(|_| ParrotError::Other("Failed to fetch playlist"))
        .ok()
        .unwrap()
        .iter()
        .map(|v| v.source_url.clone())
        .collect();

    Ok(result)
}
