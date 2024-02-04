use poise::{
    serenity_prelude::{Guild, GuildId},
    FrameworkBuilder,
};
use serenity::{cache::Settings, prelude::*};
use sqlx::{Pool, Sqlite};

pub(crate) type Error = Box<dyn std::error::Error + Send + Sync + 'static>;
pub(crate) type Context<'a> = poise::Context<'a, Data, Error>;

pub struct Data {
    pub pool: sqlx::Pool<sqlx::Sqlite>,
}

impl Data {
    pub fn new(pool: sqlx::Pool<sqlx::Sqlite>) -> Self {
        Self { pool }
    }
}

pub async fn get_framework_builder(pool: Pool<Sqlite>) -> FrameworkBuilder<Data, Error> {
    let framework_options = poise::FrameworkOptions {
        commands: vec![crate::commands::channels()],
        prefix_options: poise::PrefixFrameworkOptions {
            prefix: None,
            ..Default::default()
        },
        event_handler: |ctx, event, framework_ctx, data| {
            Box::pin(crate::logging::handle_logging_events(
                ctx,
                event,
                framework_ctx,
                data,
            ))
        },
        on_error: |error| Box::pin(on_error(error)),
        ..Default::default()
    };

    poise::Framework::builder()
        .options(framework_options)
        .setup(move |ctx, ready, framework| {
            Box::pin(async move {
                println!("Logged in as {}", ready.user.name);
                // we want to avoid creating global commands during testing cuz ratelimits are a thing.
                if cfg!(debug_assertions) {
                    let debug_guild_string = std::env::var("DEBUG_GUILD")
                            .unwrap_or_else(|_| panic!("Bot started in debug mode, but DEBUG_GUILD not set."));

                    let guild_id = GuildId::from(debug_guild_string.parse::<u64>()
                            .unwrap_or_else(|_| panic!("DEBUG_GUILD exists, but value was not a valid ID: {debug_guild_string}.")));

                    let debug_guild = Guild::get(ctx, &guild_id)
                        .await
                        .unwrap_or_else(|_| panic!("Debug guild with ID {guild_id} does not exist. Please choose a different guild, or double-check that DEBUG_GUILD is set to the right ID."));

                    println!("Using debug guild {} ({})", debug_guild.name, debug_guild.id);

                    poise::builtins::register_in_guild(ctx, &framework.options().commands, guild_id)
                    .await
                    .unwrap();
                } else {
                    poise::builtins::register_globally(ctx, &framework.options().commands)
                    .await
                    .unwrap();
                }

                Ok(Data::new(pool))
            })
        })
}

pub async fn get_client(pool: sqlx::Pool<Sqlite>) -> serenity::Client {
    let token = std::env::var("DISCORD_API_TOKEN")
        .unwrap_or_else(|_| panic!("Discord API token not present in environment. Double-check that DISCORD_API_TOKEN is set and restart."));

    let mut cache_settings = Settings::default();
    cache_settings.max_messages = 250;

    serenity::Client::builder(
        token,
        GatewayIntents::GUILD_MEMBERS
            | GatewayIntents::GUILD_MESSAGES
            | GatewayIntents::MESSAGE_CONTENT,
    )
    .cache_settings(cache_settings)
    .framework(get_framework_builder(pool).await.build())
    .await
    .unwrap()
}

async fn on_error(error: poise::FrameworkError<'_, Data, Error>) {
    println!("{error}");
}
