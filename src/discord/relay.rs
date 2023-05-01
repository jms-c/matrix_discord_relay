use reqwest;
use std::collections::HashMap;
use serde::Deserialize;
use crate::CONFIG;
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