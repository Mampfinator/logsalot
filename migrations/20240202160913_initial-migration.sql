CREATE TABLE IF NOT EXISTS log_channels (
    guild_id TEXT PRIMARMY KEY NOT NULL,
    member_logs TEXT,
    chat_logs TEXT,
    server_logs TEXT
);