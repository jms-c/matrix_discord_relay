use crate::chat_service::{FullMessage, Message};
use crate::{chat_service, CONFIG};
use reqwest;
use serde::Deserialize;
use std::collections::HashMap;
use std::fmt::format;

use super::bot::{CONTEXT, relayed_message_to_message};

#[derive(Debug, Deserialize, Clone)]
struct WebhookResponse {
    id: String,
    channel_id: String,
}

fn sanitize(message: String) -> String
{
    let mut out = message.clone();
    let zero_width_space = "​";
    let fake_ping = format!("@{}", zero_width_space);
    out = out.replace("@everyone", format!("{}everyone", fake_ping).as_str());
    out = out.replace("@here", format!("{}here", fake_ping).as_str());
    return out;
}

pub async fn delete_message(message: Message)
{
    let relayed_messages = chat_service::message_relays(message.clone());
    let http = (*(CONTEXT.lock().unwrap())).as_ref().unwrap().http.clone();
    if relayed_messages.len() > 0 {
        for msg in relayed_messages {
            if msg.service == "discord" {
                let discord_msg = relayed_message_to_message(msg).await;
                if discord_msg.is_some() {
                    discord_msg.unwrap().delete(http.clone()).await;
                }
            }
        }
    }

    let origin_message = chat_service::message_origin(message.clone());
    if origin_message.is_some() && origin_message.clone().unwrap().clone().service == "discord" {
        let discord_msg = relayed_message_to_message(origin_message.unwrap()).await;
        if discord_msg.is_some() {
            discord_msg.unwrap().delete(http.clone()).await;
        }
    }
}


async fn send_message_webhook(
    webhook: String,
    message: String,
    username: Option<String>,
) -> WebhookResponse {
    let mut params = HashMap::new();
    params.insert("content", sanitize(message));
    if username.is_some() {
        params.insert("username", username.unwrap());
    }

    let client = reqwest::Client::new();
    let res = client
        .post(format!("{}?wait=1", webhook))
        .form(&params)
        .send()
        .await
        .expect("Should have sent message!")
        .json::<WebhookResponse>()
        .await
        .expect("Should have parsed!");

    return res;
}

async fn edit_message_webhook(
    webhook: String,
    message_id: String,
    message: String,
) -> WebhookResponse {
    let mut params = HashMap::new();
    params.insert("content", sanitize(message));

    let client = reqwest::Client::new();
    let res = client
        .patch(format!("{}/messages/{}", webhook, message_id))
        .form(&params)
        .send()
        .await
        .expect("Should have sent message!")
        .json::<WebhookResponse>()
        .await
        .expect("Should have parsed!");

    return res;
}

pub async fn relay_message(message: FullMessage) -> Message {
    let mut out: Message = message.message.clone();
    let mut webhook = "".to_owned();
    let room = CONFIG
        .room
        .iter()
        .find(|room| room.matrix == message.message.room_id);
    if room.is_none() {
        return message.message;
    }

    out = Message {
        service: "discord".to_owned(),
        server_id: room.unwrap().discord_guild.to_owned(),
        room_id: room.unwrap().discord.clone(),
        id: message.message.id.clone(),
    };
    webhook = room.unwrap().webhook.clone();

    let wh = send_message_webhook(
        webhook,
        message.content,
        Some(format!("{} ({})", message.user.display, message.user.tag).to_owned()),
    )
    .await;
    out.id = wh.id;
    return out;
}

pub async fn edit_message(message: FullMessage) {
    let room = CONFIG.room.iter().find(|room| room.matrix == message.message.room_id);
    let webhook = room.unwrap().webhook.clone();

    let relayed_messages = chat_service::message_relays(message.clone().message);
    for msg in relayed_messages {
        if msg.service == "discord" {
            edit_message_webhook(webhook.clone(), msg.id, message.clone().content).await;
        }
    }
}
