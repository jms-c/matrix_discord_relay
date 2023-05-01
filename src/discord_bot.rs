use std::env;

use serenity::model::prelude::{ChannelId, MessageId};
use serenity::{async_trait, model::prelude::GuildId};
use serenity::model::channel::Message;
use serenity::model::gateway::Ready;
use serenity::prelude::*;

use crate::{matrix};
use crate::{CONFIG, chat_service::{self, FullMessage, User}};

struct Handler;

lazy_static! {
    pub static ref CONTEXT: std::sync::Mutex<Option<Context>> = std::sync::Mutex::new(None);
}

// I pass guild id as argument as replies do not have guild id correctly set
fn message_to_relayed_message(msg: Message, guild_id: String) -> chat_service::Message
{
    let relay_msg = chat_service::Message {
        service: "discord".to_owned(),
        id: msg.id.to_string(),
        room_id: msg.channel_id.to_string(),
        server_id: guild_id,
    };
    return relay_msg;
}

async fn message_to_full_message(msg: Message) -> chat_service::FullMessage {
    let ctx = (*(CONTEXT.lock().unwrap())).clone().unwrap();
    let mut display_name = msg.clone().author.name;
    let nick = msg.clone().author_nick(ctx.http.clone()).await.clone();
    if nick.is_some() {
        display_name = nick.unwrap().to_owned();
    }
    let user = User {
        source: "discord".to_string(), // Source, e.g matrix, discord
        id: msg.author.id.to_string(), // Actual id
        ping: format!("<@{}>", msg.clone().author.id.to_string()), // Used to mention user
        tag: format!("{}", msg.clone().author.tag()), // Used to tag (kinda)
        display: display_name // Display Name
    };

    let relay_msg = message_to_relayed_message(msg.clone(), msg.guild_id.unwrap().to_string());

    let mut reply: Option<Box<chat_service::Message>> = None;
    if msg.referenced_message.is_some() {
        let replyed_msg = *(msg.referenced_message.unwrap());
        reply = Some(Box::new(message_to_relayed_message(replyed_msg, msg.guild_id.unwrap().to_string())));
    }

    let full_msg = FullMessage {
        user: user,
        message: relay_msg,
        content: msg.content.clone(),
        reply: reply
    };

    return full_msg;
}

pub async fn relayed_message_to_message(msg: chat_service::Message) -> Message {
    // This may or may not work...
    let ctx = (*(CONTEXT.lock().unwrap())).clone().unwrap();
    let guild_id = GuildId(msg.server_id.parse::<u64>().unwrap());
    let channel_id = ChannelId(msg.room_id.parse::<u64>().unwrap());
    let channels = guild_id.channels(ctx.http.clone()).await;
    let channel = channels.as_ref().unwrap().get(&channel_id).unwrap();

    let message_id = MessageId(msg.id.parse::<u64>().unwrap());
    return channel.message(ctx.http.clone(), message_id).await.unwrap();
}

#[async_trait]
impl EventHandler for Handler {
    // Set a handler for the `message` event - so that whenever a new message
    // is received - the closure (or function) passed will be called.
    //
    // Event handlers are dispatched through a threadpool, and so multiple
    // events can be dispatched simultaneously.
    async fn message(&self, ctx: Context, msg: Message) {
        println!("{} {} {}", msg.content, msg.id, msg.author.bot);
        if msg.author.bot {
            return;
        }

        for m in CONFIG.room.iter() {
            if m.discord == msg.channel_id.to_string() {
                matrix::bot::relay_message(message_to_full_message(msg).await).await;
                
                //chat_service::create_message(relay_msg, relayed);

                break;
            }
        }
        //message_handler::relay_message()
    }

    // Set a handler to be called on the `ready` event. This is called when a
    // shard is booted, and a READY payload is sent by Discord. This payload
    // contains data like the current user's guild Ids, current user data,
    // private channels, and more.
    //
    // In this case, just print what the current user's username is.
    async fn ready(&self, ctx: Context, ready: Ready) {
        (*(CONTEXT.lock().unwrap())) = Some(ctx.clone());
        println!("{} is connected!", ready.user.name);
    }
}

pub async fn start_bot() {
    // Configure the client with your Discord bot token in the environment.
    //let token = env::var("DISCORD_TOKEN").expect("Expected a token in the environment");
    let token = CONFIG.discord_token.clone();
    // Set gateway intents, which decides what events the bot will be notified about
    let intents = GatewayIntents::GUILD_MESSAGES
        | GatewayIntents::DIRECT_MESSAGES
        | GatewayIntents::MESSAGE_CONTENT;

    // Create a new instance of the Client, logging in as a bot. This will
    // automatically prepend your bot token with "Bot ", which is a requirement
    // by Discord for bot users.
    let mut client =
        Client::builder(&token, intents).event_handler(Handler).await.expect("Err creating client");


    // Finally, start a single shard, and start listening to events.
    //
    // Shards will automatically attempt to reconnect, and will perform
    // exponential backoff until it reconnects.
    if let Err(why) = client.start().await {
        println!("Client error: {:?}", why);
    }
}