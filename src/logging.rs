use poise::FrameworkContext;
use serenity::{
    all::{client::Context, FullEvent, GuildId, User},
    builder::{
        CreateAllowedMentions, CreateAttachment, CreateEmbed, CreateEmbedAuthor, CreateMessage,
    },
    model::Colour,
};
use std::hash::Hash;
use std::{collections::HashSet, fmt::Display};

use crate::{client::Data, commands::LogType};

fn display_name(user: &User) -> String {
    let nick = user
        .member
        .as_ref()
        .and_then(|member| member.nick.clone())
        .or_else(|| user.global_name.clone());
    let name = format!("@{}", user.name.clone());

    if let Some(nick) = nick {
        format!("{name} ({nick})")
    } else {
        name
    }
}

fn base_embed(user: &User) -> CreateEmbed {
    CreateEmbed::new().author(
        CreateEmbedAuthor::new(display_name(user)).icon_url(
            user.avatar_url()
                .unwrap_or_else(|| user.default_avatar_url()),
        ),
    )
}

#[derive(Clone, Debug)]
struct AsymmetricDiff<T: PartialEq + Hash + Clone> {
    added: HashSet<T>,
    removed: HashSet<T>,
}

fn asymmetric_diff(from: Vec<String>, to: Vec<String>) -> AsymmetricDiff<String> {
    let mut removed = from.clone().into_iter().collect::<HashSet<_>>();
    let mut added = to.clone().into_iter().collect::<HashSet<_>>();

    for item in from.iter() {
        added.remove(item);
    }

    for item in to.iter() {
        removed.remove(item);
    }

    AsymmetricDiff { removed, added }
}

fn pluralize<'a>(singular: &'a str, plural: &'a str, count: usize) -> &'a str {
    match count {
        1 => singular,
        _ => plural,
    }
}

async fn make_embed(
    ctx: &Context,
    event: &FullEvent,
    _framework_ctx: FrameworkContext<'_, Data, crate::client::Error>,
    _data: &Data,
) -> Option<(CreateMessage, LogType, GuildId, Option<Vec<CreateMessage>>)> {
    match event {
        FullEvent::MessageDelete {
            channel_id,
            deleted_message_id,
            guild_id,
        } => {
            let guild_id = *(guild_id.as_ref()?);
            let message = ctx.cache.message(channel_id, deleted_message_id)?.clone();

            if message.author.bot {
                return None;
            }

            let message_content = if !message.content.is_empty() {
                message.content
            } else {
                "None".into()
            };

            let mut followups = Vec::new();

            let timestamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs();

            let mut log_embed = base_embed(&message.author)
                .colour(Colour::RED)
                .description(format!(
                    "A message by <@{}> (**{}**) was deleted in <#{}>.",
                    message.author.id, message.author.name, message.channel_id
                ))
                .field("Content", message_content, false)
                .field("Timestamp", format!("<t:{}>", timestamp), true);

            let mut log_message = CreateMessage::new();

            if !message.attachments.is_empty() {
                log_embed = log_embed.field(
                    "No. Attachments",
                    format!("{}", message.attachments.len()),
                    true,
                );

                let mut followup_message = CreateMessage::new();

                for attachment in message.attachments.iter() {
                    let attachment_builder =
                        CreateAttachment::url(ctx, &attachment.url).await.unwrap();

                    followup_message = followup_message.add_file(attachment_builder);
                }

                followups.push(followup_message);
            }

            log_message = log_message.embed(log_embed);

            Some((log_message, LogType::Chat, guild_id, Some(followups)))
        }
        FullEvent::MessageUpdate {
            old_if_available,
            new,
            event: _,
        } => {
            let old = old_if_available.as_ref()?.clone();

            if old.author.bot {
                return None;
            }

            let guild_id = old.guild_id?;
            let new = new.as_ref()?.clone();

            let mut followups = Vec::new();

            let mut description = format!(
                "<@{}> (**{}**) updated their message in <#{}>.\n [Jump to message]({})",
                new.author.id,
                new.author.name,
                new.channel_id,
                new.link()
            );

            let mut log_embed = base_embed(&old.author).colour(Colour::FADED_PURPLE);

            let content_changed = old.content != new.content;

            if content_changed {
                log_embed = log_embed.field("New", new.content, false).field(
                    "Previous",
                    old.content,
                    false,
                );
            } else {
                description += "\n\n Message content hasn't changed. Check followup message(s) for attachment changes."
            }

            let timestamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs();
            log_embed = log_embed.field("Timestamp", format!("<t:{}>", timestamp), true);

            let attachments_could_have_changed =
                !old.attachments.is_empty() || !new.attachments.is_empty();

            if attachments_could_have_changed {
                let difference = asymmetric_diff(
                    old.attachments.iter().map(|a| a.url.clone()).collect(),
                    new.attachments.iter().map(|a| a.url.clone()).collect(),
                );

                log_embed = log_embed.field(
                    "Attachments",
                    format!(
                        "**Removed**: {} | **Added**: {}",
                        difference.removed.len(),
                        difference.added.len()
                    ),
                    true,
                );

                if !difference.added.is_empty() {
                    let mut added_attachments = CreateMessage::new().content(format!(
                        "Added {}:",
                        pluralize("attachment", "attachments", difference.added.len())
                    ));

                    for url in difference.added.iter() {
                        added_attachments =
                            added_attachments.add_file(CreateAttachment::url(ctx, url).await.ok()?);
                    }

                    followups.push(added_attachments);
                }

                if !difference.removed.is_empty() {
                    let mut removed_attachments = CreateMessage::new().content(format!(
                        "Removed {}:",
                        pluralize("attachment", "attachments", difference.removed.len())
                    ));

                    for url in difference.removed.iter() {
                        removed_attachments = removed_attachments
                            .add_file(CreateAttachment::url(ctx, url).await.ok()?);
                    }

                    followups.push(removed_attachments);
                }
            }

            // slightly hacky workaround - we don't want to log embed deletions (yet).
            if content_changed || attachments_could_have_changed {
                Some((
                    CreateMessage::new().embed(log_embed.description(description)),
                    LogType::Chat,
                    guild_id,
                    Some(followups),
                ))
            } else {
                None
            }
        }
        // USERS
        FullEvent::GuildMemberAddition { new_member: member } => {
            let embed = base_embed(&member.user)
                .colour(Colour::DARK_GREEN)
                .description(format!(
                    "<@{}> ({}) joined.",
                    member.user.id, member.user.name
                ))
                .field(
                    "Joined At",
                    format!("<t:{}:R>", member.joined_at?.timestamp()),
                    true,
                )
                .field(
                    "Created At",
                    format!("<t:{}:R>", member.user.created_at().timestamp()),
                    true,
                );

            Some((
                CreateMessage::new().embed(embed),
                LogType::Member,
                member.guild_id,
                None,
            ))
        }
        FullEvent::GuildMemberRemoval {
            guild_id,
            user,
            member_data_if_available,
        } => {
            // TODO: shit's fucked. Members are not gonna be cached. We may be able to fetch guilds on startup?
            let member = member_data_if_available.as_ref()?;

            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs();

            let embed = base_embed(user)
                .colour(Colour::DARK_RED)
                .description(format!("<@{}> ({}) left.", user.id, user.name))
                .field(
                    "Joined At",
                    format!("<t:{}:R>", member.joined_at?.timestamp()),
                    true,
                )
                .field(
                    "Created At",
                    format!("<t:{}:R>", user.created_at().timestamp()),
                    true,
                )
                .field("Left At", format!("<t:{}:R>", now), true);

            Some((
                CreateMessage::new().embed(embed),
                LogType::Member,
                *guild_id,
                None,
            ))
        }
        FullEvent::GuildMemberUpdate {
            old_if_available,
            new: _,
            event: _,
        } => {
            let _old = old_if_available.as_ref()?;

            None
        }
        _ => None,
    }
}

#[derive(Debug, Copy, Clone)]
struct NoLogChannelSet {
    #[allow(unused)]
    pub log_type: LogType,
    #[allow(unused)]
    pub guild_id: GuildId,
}

impl Display for NoLogChannelSet {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Debug::fmt(self, f)
    }
}

impl std::error::Error for NoLogChannelSet {}

pub async fn handle_logging_events(
    ctx: &Context,
    event: &FullEvent,
    framework_ctx: FrameworkContext<'_, Data, crate::client::Error>,
    data: &Data,
) -> Result<(), crate::client::Error> {
    let payload = make_embed(ctx, event, framework_ctx, data).await;

    if let Some((message, log_type, guild_id, followups)) = payload {
        let channel = log_type
            .fetch_channel(&data.pool, guild_id)
            .await
            .ok_or(NoLogChannelSet { log_type, guild_id })?;

        let message = channel.send_message(ctx, message).await?;

        if let Some(followups) = followups
            && !followups.is_empty()
        {
            for followup in followups.into_iter() {
                channel
                    .send_message(
                        ctx,
                        followup
                            .reference_message(&message)
                            .allowed_mentions(CreateAllowedMentions::new().empty_users()),
                    )
                    .await?;
            }
        }
    }

    Ok(())
}
