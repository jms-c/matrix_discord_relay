use reqwest;
use std::collections::HashMap;
use serde::Deserialize;
use crate::{CONFIG, chat_service};
use crate::chat_service::{Message, FullMessage};


#[derive(Debug, Deserialize, Clone)]
struct WebhookResponse {
    id: String,
    channel_id: String
}

async fn send_message_webhook(webhook: String, message: String, username: Option<String>) -> WebhookResponse
{
    let mut params = HashMap::new();
    params.insert("content", message);
    if username.is_some() {
        params.insert("username", username.unwrap());
    }

    let client = reqwest::Client::new();
    let res = client.post(format!("{}?wait=1",webhook))
    .form(&params)
    .send().await
    .expect("Should have sent message!")
    .json::<WebhookResponse>().await.expect("Should have parsed!");

    return res;
}

async fn edit_message_webhook(webhook: String, message_id: String, message: String) -> WebhookResponse
{
    let mut params = HashMap::new();
    params.insert("content", message);

    let client = reqwest::Client::new();
    let res = client.patch(format!("{}/messages/{}",webhook, message_id))
    .form(&params)
    .send().await
    .expect("Should have sent message!")
    .json::<WebhookResponse>().await.expect("Should have parsed!");

    return res;
}

pub async fn relay_message(message: FullMessage) -> Message
{
    let mut out: Message = message.message.clone();
    let mut webhook = "".to_owned();
    for mroom in CONFIG.room.iter() {
        if mroom.matrix == message.message.room_id
        {
            out = Message {
                service: "discord".to_owned(),
                server_id: mroom.discord_guild.to_owned(),
                room_id: mroom.discord.clone(),
                id: message.message.id.clone()
            };
            webhook = mroom.webhook.clone();
            break;
        }
    }

    let wh = send_message_webhook(webhook, message.content, Some(format!("{} ({})", message.user.display, message.user.tag).to_owned())).await;
    out.id = wh.id;
    return out; 
}

pub async fn edit_message(message: FullMessage)
{
    let mut webhook = "".to_owned();
    for mroom in CONFIG.room.iter() {
        if mroom.matrix == message.message.room_id
        {
            webhook = mroom.webhook.clone();
            break;
        }
    }

    let relayed_messages = chat_service::message_relays(message.clone().message);
    for msg in relayed_messages {
        if msg.service == "discord"
        {
            edit_message_webhook(webhook.clone(), msg.id, message.clone().content).await;
        }
    }
}