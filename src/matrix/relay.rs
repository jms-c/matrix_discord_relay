use std::{f32::consts::E, thread::panicking};

use futures::future::Join;
use matrix_sdk::{Client, room::Joined};
use ruma::{RoomId, events::{room::message::{RoomMessageEventContent, Relation, MessageType}, relation::{InReplyTo, Replacement}}, EventId, OwnedEventId, MxcUri};

use crate::{chat_service::{Message, FullMessage, self}, CONFIG};

use super::bot::{BOT_REGISTRATION, BOT_APPSERVICE, BOT_CLIENT};

async fn get_room_as_user(user: Client, room_id: &RoomId) -> Joined
{
    let client_local =  (*(BOT_CLIENT.lock().expect("Bot client is poisoned"))).clone();
    let appservice_room = client_local.unwrap().get_joined_room(room_id);
    appservice_room.unwrap().invite_user_by_id(user.user_id().unwrap()).await;

    user.join_room_by_id(room_id).await;
    return user.get_joined_room(room_id).unwrap();
}

async fn get_bot_user(user_id: String) -> Client
{
    let registration_local = (*(BOT_REGISTRATION.lock().expect("Bot registration is poisoned"))).clone();
    let appservice_local = (*(BOT_APPSERVICE.lock().expect("Bot appservice is poisoned"))).clone();

    let relay_bot_name = format!(
        "{}{}",
        registration_local
            .as_ref()
            .unwrap()
            .sender_localpart
            .clone(),
        user_id
    );

    let res = appservice_local
        .as_ref()
        .unwrap()
        .register_user(&relay_bot_name, None)
        .await;    

    let user = appservice_local
        .as_ref()
        .unwrap()
        .user(Some(&relay_bot_name))
        .await.unwrap();
    return user;
}

pub async fn relay_message(message: FullMessage) -> Message
{
    let mut out: Message = message.message.clone();
    for mroom in CONFIG.room.iter() {
        if mroom.discord == message.message.room_id
        {
            out = Message {
                service: "matrix".to_owned(),
                server_id: "".to_owned(),
                room_id: mroom.matrix.clone(),
                id: message.message.id.clone()
            };
            break;
        }
    }

    let user = get_bot_user(message.user.id).await;

    let changed_name = user
        .account()
        .set_display_name(Some(format!("{} ({})", &message.user.display.clone(), &message.user.tag.clone()).as_str()))
        .await
        .is_ok();

    if message.user.avatar.is_some() {
        //user.account().set_avatar_url(uri);
    }


    let id: Box<RoomId> = RoomId::parse_box(out.room_id.clone().as_ref()).unwrap();

    let room = get_room_as_user(user, id.as_ref()).await;
    let content = RoomMessageEventContent::text_html(message.content.clone(), markdown::to_html(&message.content.clone()));

    let mut reply_id: String = "".to_owned();
    if message.reply.is_some() {
        let reply_msg = *message.reply.unwrap();

        let relayed_messages = chat_service::message_relays(reply_msg.clone());
        
        if relayed_messages.len() > 0 {
            for msg in relayed_messages.iter() {
                if msg.service == "matrix" {
                    reply_id = msg.id.clone();
                }
            }
        }
        else {
            let origin_message = chat_service::message_origin(reply_msg.clone());
            if origin_message.is_some() {
                reply_id = origin_message.unwrap().id;
            }
            else
            {
                panic!("Cursed reply!");
            }
        }

    }

    if reply_id == "" {
        let res = room.send(content, None).await;
        out.id = res.unwrap().event_id.to_string();
    }
    else {
        let res = reply_to_message(room, EventId::parse(reply_id).unwrap(), content).await;
        out.id = res.to_string();
    }
    //let member = room.get_member(&user.user_id().unwrap()).await.unwrap().unwrap().
    return out;
}

pub async fn edit_message(message: FullMessage)
{
    let html_body = markdown::to_html(&message.content.clone());
    let body = message.content.clone();
    let content = RoomMessageEventContent::text_html(body.clone(), html_body.clone());
    let relayed_messages = chat_service::message_relays(message.message);
    let user = get_bot_user(message.user.id).await;
    
    for msg in relayed_messages.iter() {
        if msg.service == "matrix" {
            let id: Box<RoomId> = RoomId::parse_box(msg.room_id.clone().as_ref()).unwrap();
            let room = get_room_as_user(user.clone(), id.as_ref()).await;
            let event_id = EventId::parse(msg.id.clone()).unwrap();

            let replacement = Replacement::new(
                event_id,
                MessageType::text_html(body.clone(), html_body.clone())
            );
            let mut edited_content = content.clone();
            edited_content.relates_to = Some(Relation::Replacement(replacement));
            room.timeline().await.send(edited_content.into(), None).await;
        }
    }
}

pub async fn delete_message(message: Message)
{

    let relayed_messages = chat_service::message_relays(message);
    if relayed_messages.len() > 0 {
        for msg in relayed_messages {
            if msg.service == "matrix" {
                let id: Box<RoomId> = RoomId::parse_box(msg.room_id.clone().as_ref()).unwrap();
                
                let client_local =  (*(BOT_CLIENT.lock().expect("Bot client is poisoned"))).clone();
                let appservice_room = client_local.unwrap().get_joined_room(id.as_ref());
            
                let event_id = EventId::parse_box(msg.id).unwrap();
                let event_id_ref = &(*event_id);
                appservice_room.unwrap().redact(event_id_ref, None, None).await;
            }
        }
    }
}

async fn reply_to_message(room: Joined, event_id: OwnedEventId, content: RoomMessageEventContent) -> OwnedEventId
{
    let replacement = InReplyTo::new(
        event_id
    );
    let mut reply_content = content;
    reply_content.relates_to = Some(Relation::Reply { in_reply_to: replacement } );
    let timeline = room.timeline().await;

    let out = timeline.send(reply_content.into(), None).await;
    return timeline.latest_event().unwrap().as_local().unwrap().event_id().unwrap().to_owned();
}