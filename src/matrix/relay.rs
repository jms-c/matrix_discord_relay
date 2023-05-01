use matrix_sdk::{Client, room::Joined};
use ruma::{RoomId, events::{room::message::{RoomMessageEventContent, Relation}, relation::InReplyTo}, EventId, OwnedEventId};

use crate::{chat_service::{Message, FullMessage, self}, CONFIG};

use super::bot::{BOT_REGISTRATION, BOT_APPSERVICE};

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

    let registration_local = (*(BOT_REGISTRATION.lock().expect("Bot registration is poisoned"))).clone();
    let appservice_local = (*(BOT_APPSERVICE.lock().expect("Bot appservice is poisoned"))).clone();

    let relay_bot_name = format!(
        "{}{}",
        registration_local
            .as_ref()
            .unwrap()
            .sender_localpart
            .clone(),
        message.user.id
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


    let changed_name = user
        .account()
        .set_display_name(Some(format!("{} ({})", &message.user.display.clone(), &message.user.tag.clone()).as_str()))
        .await
        .is_ok();


    let id: Box<RoomId> = RoomId::parse_box(out.room_id.clone().as_ref()).unwrap();
    user.join_room_by_id(id.as_ref()).await.unwrap();


    let room = user.get_joined_room(id.as_ref()).unwrap();
    let content = RoomMessageEventContent::text_html(message.content.clone(), markdown::to_html(&message.content.clone()));

    let mut reply_id: String = "".to_owned();
    if message.reply.is_some() {
        let reply_msg = (*message.reply.unwrap());

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