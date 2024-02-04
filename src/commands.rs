use std::str::FromStr;

use poise::serenity_prelude::*;
use sqlx::{prelude::*, Pool, Sqlite};

use crate::client::{Context, Error};

#[derive(FromRow)]
struct LogChannels {
    guild_id: String,
    member_logs: Option<String>,
    chat_logs: Option<String>,
    server_logs: Option<String>,
}

impl LogChannels {
    pub fn guild_id(&self) -> GuildId {
        GuildId::from_str(&self.guild_id).unwrap() // this should *never* be an invalid guild ID.
    }

    pub fn member_logs(&self) -> Option<ChannelId> {
        self.member_logs
            .as_ref()
            .and_then(|id| ChannelId::from_str(id).ok())
    }

    pub fn chat_logs(&self) -> Option<ChannelId> {
        self.chat_logs
            .as_ref()
            .and_then(|id| ChannelId::from_str(id).ok())
    }

    pub fn server_logs(&self) -> Option<ChannelId> {
        self.server_logs
            .as_ref()
            .and_then(|id| ChannelId::from_str(id).ok())
    }

    pub fn new(guild_id: GuildId) -> Self {
        Self {
            guild_id: guild_id.to_string(),
            member_logs: None,
            chat_logs: None,
            server_logs: None,
        }
    }

    pub async fn insert_default(pool: &Pool<Sqlite>, guild_id: String) {
        sqlx::query!(
            "INSERT INTO log_channels (guild_id) VALUES (?) ON CONFLICT DO NOTHING",
            guild_id
        )
        .execute(pool)
        .await
        .unwrap();
    }
}

#[poise::command(
    slash_command,
    subcommands("list", "set"),
    guild_only,
    default_member_permissions = "MANAGE_CHANNELS"
)]
pub async fn channels(_: Context<'_>) -> Result<(), Error> {
    Ok(())
}

#[derive(Debug, poise::ChoiceParameter, Clone, Copy)]
pub enum LogType {
    #[name = "Member Logs"]
    Member,
    #[name = "Chat Logs"]
    Chat,
    #[name = "Server Logs"]
    Server,
}

impl LogType {
    pub(crate) fn as_column_name(&self) -> &str {
        match self {
            Self::Member => "member_logs",
            Self::Chat => "chat_logs",
            Self::Server => "server_logs",
        }
    }

    pub(crate) async fn fetch_channel(
        &self,
        pool: &Pool<Sqlite>,
        guild_id: GuildId,
    ) -> Option<ChannelId> {
        let column_name = self.as_column_name();

        let row = sqlx::query(&format!(
            "SELECT {column_name} FROM log_channels WHERE guild_id = ?"
        ))
        .bind(guild_id.to_string())
        .fetch_optional(pool)
        .await
        .ok()??;

        let id: &str = row.get(column_name);

        ChannelId::from_str(id).ok()
    }
}

impl ToString for LogType {
    fn to_string(&self) -> String {
        match self {
            Self::Member => "Member Logs".into(),
            Self::Chat => "Chat Logs".into(),
            Self::Server => "Server Logs".into(),
        }
    }
}

#[poise::command(slash_command)]
async fn set(
    ctx: Context<'_>,
    log_type: LogType,
    #[channel_types("Text")] channel: Option<ChannelId>,
) -> Result<(), Error> {
    use LogType as C;

    let pool = &ctx.data().pool;

    let guild_id = ctx.guild_id().unwrap().to_string();
    let value = channel.map(|id| id.to_string());

    (match log_type {
        C::Member => {
            sqlx::query!(
                "UPDATE log_channels SET member_logs = ? WHERE guild_id = ?",
                value,
                guild_id
            )
        }
        C::Chat => {
            sqlx::query!(
                "UPDATE log_channels SET chat_logs = ? WHERE guild_id = ?",
                value,
                guild_id
            )
        }
        C::Server => {
            sqlx::query!(
                "UPDATE log_channels SET server_logs = ? WHERE guild_id = ?",
                value,
                guild_id
            )
        }
    })
    .execute(pool)
    .await?;

    match value {
        None => ctx.reply(format!("Unset {}", log_type.to_string())),
        Some(channel_id) => ctx.reply(format!(
            "{} will now be sent to <#{}>",
            log_type.to_string(),
            channel_id
        )),
    }
    .await
    .unwrap();

    Ok(())
}

#[poise::command(slash_command)]
async fn list(ctx: Context<'_>) -> Result<(), Error> {
    let pool = &ctx.data().pool;

    let guild_id = ctx.guild_id().unwrap();
    let guild_id_string = guild_id.to_string();

    let log_channels = match sqlx::query_as!(
        LogChannels,
        "SELECT * FROM log_channels WHERE guild_id = ?",
        guild_id_string
    )
    .fetch_optional(pool)
    .await?
    {
        Some(channels) => channels,
        None => {
            LogChannels::insert_default(pool, guild_id_string).await;
            LogChannels::new(guild_id)
        }
    };

    let guild_name = log_channels.guild_id().name(ctx).unwrap();

    ctx.reply(format!(
        "Log channels for {guild_name}\nMember logs: <#{}>\nChat logs: <#{}>\nServer logs: <#{}>",
        log_channels
            .member_logs()
            .map(|id| id.to_string())
            .unwrap_or("None".into()),
        log_channels
            .chat_logs()
            .map(|id| id.to_string())
            .unwrap_or("None".into()),
        log_channels
            .server_logs()
            .map(|id| id.to_string())
            .unwrap_or("None".into())
    ))
    .await?;

    Ok(())
}
