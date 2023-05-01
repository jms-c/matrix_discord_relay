use std::{cell::RefCell, env, string, sync::Mutex};

use futures::future;
use ruma::{
    api::client::appservice,
    api::{appservice::Registration, client::error::ErrorKind},
    events::room::message::{RoomMessageEvent, TextMessageEventContent},
    events::{
        room::{message::{OriginalSyncRoomMessageEvent, RoomMessageEventContent, MessageType, Relation}, member::RoomMemberEventContent},
        AnyMessageLikeEventContent, OriginalSyncMessageLikeEvent, relation::{Replacement, InReplyTo}, StateEventContent, AnyTimelineEvent,
    },
    room_id, OwnedRoomId, RoomId, RoomOrAliasId, EventId,
};

use matrix_sdk_appservice::{
    matrix_sdk::{
        config::SyncSettings,
        event_handler::Ctx,
        room::Room,
        ruma::{
            events::room::member::{MembershipState, OriginalSyncRoomMemberEvent},
            UserId,
        },
        Client, sync::SyncResponse,
    },
    AppService, AppServiceBuilder, AppServiceRegistration, Result,
};
use tracing::{info, trace};
use tracing_subscriber::fmt::format::{self, Full};

use crate::{
    chat_service::{self, Message, User, FullMessage},
    CONFIG, discord,
};

pub static BOT_APPSERVICE: Mutex<Option<AppService>> = Mutex::new(None);
pub static BOT_REGISTRATION: Mutex<Option<AppServiceRegistration>> = Mutex::new(None);
pub static BOT_CLIENT: Mutex<Option<Client>> = Mutex::new(None);

fn find_ping(ping: String) -> String {
    let user = ping.trim_start_matches("<").trim_start_matches("@").trim_end_matches(">");
    let parts = user.split(":").collect::<Vec<&str>>();
    let localpart = parts[0].to_owned();

    let registration_local = (*(BOT_REGISTRATION.lock().unwrap())).clone().unwrap();
    let bot_localpart = registration_local.sender_localpart.clone().to_owned();

    if localpart.starts_with(bot_localpart.as_str()) {
        return format!("<@{}>", &localpart[bot_localpart.len()..]);
    }
    else
    {
        return ping.trim_start_matches("<").trim_end_matches(">").to_owned();
    }
}

fn strip_reply(msg: String) -> String
{
    let mut actual_message = "".to_owned();

    for line in msg.lines() {
        if (!line.starts_with("> ") && line != "") || actual_message != "" {
            actual_message = format!("{}{}\n", actual_message, line).to_owned();
            continue;
        }
    }
    return actual_message;
}

async fn handle_room_message(event: OriginalSyncRoomMessageEvent, room: Room) {
    println!("GOT MESSAGE");
    println!("{}", event.content.body());

    let registration_local = (*(BOT_REGISTRATION.lock().unwrap())).clone().unwrap();
    let bot_localpart = registration_local.sender_localpart.clone();
    println!("Handled message!");
    if event.sender.localpart().starts_with(&bot_localpart) {
        return;
    }

    if let Room::Joined(room) = room {

        let mut continue_relay = false;
        for m in CONFIG.room.iter() {
            if m.matrix == room.room_id().to_string() {
                continue_relay = true;
            }
        }
        if !continue_relay {
            return;
        }

        let msg = Message {
            service: "matrix".to_owned(),
            server_id: "".to_owned(),
            room_id: room.room_id().to_string(),
            id: event.event_id.to_string()
        };

        let user = User {
            source: "matrix".to_owned(),
            id: event.sender.to_string(),
            ping: format!("<@{}>", event.sender.to_string()),
            tag: event.sender.to_string(),
            display: event.sender.to_string(),
            avatar: None
        };

        let mut relay_msg = FullMessage {
            message: msg,
            user: user,
            content: event.content.body().to_string(),
            reply: None
        };
        //let content = RoomMessageEventContent::text_plain("🎉🎊🥳 let's PARTY!! 🥳🎊🎉");

        println!("sending");

        // Very temporary reply system (I need to find replied message from event.content.relates_to)
        if event.content.relates_to.is_some() {
            let mut reply_header = "".to_owned();

            match event.content.clone().relates_to.unwrap() {
                Relation::Reply {in_reply_to} => {
                    let reply_id = in_reply_to.event_id;
                    let reply_data = room.event(&reply_id).await.unwrap().event.json().to_string();
                    let v: serde_json::Value = serde_json::from_str(&reply_data).unwrap();

                    let mut reply_body = v["content"]["body"].as_str().unwrap().to_owned();
                    reply_body = strip_reply(reply_body);

                    let reply_author = v["sender"].as_str().unwrap().to_owned();
                    let author_ping = find_ping(reply_author);

                    let mut header = reply_body.lines().collect::<Vec<&str>>()[0].to_owned();
                    if header.len() > 64 {
                        header = format!("{}...", &header[..64]);
                    }

                    let reply_msg = Message { service: "matrix".to_owned(), server_id: "".to_owned(), room_id: room.room_id().to_string(), id: reply_id.to_string() };

                    let relayed_messages = chat_service::message_relays(reply_msg.clone());
                    let mut discord_msg_url = "".to_owned();
                    for msg in relayed_messages {
                        if msg.service == "discord" {
                            discord_msg_url = format!("https://discord.com/channels/{}/{}/{}", msg.server_id, msg.room_id, msg.id);
                        }
                    }
                    let origin_message = chat_service::message_origin(reply_msg.clone());
                    if origin_message.is_some() {
                        if origin_message.clone().unwrap().service == "discord" {
                            discord_msg_url = format!("https://discord.com/channels/{}/{}/{}", origin_message.clone().unwrap().server_id, origin_message.clone().unwrap().room_id, origin_message.clone().unwrap().id);
                        }
                    }

                    if discord_msg_url != "" {
                        header = format!("[{}]({})", header, discord_msg_url).to_owned();
                    }
                    //https://discord.com/channels/server/channel/msg
                    reply_header = format!("> {} {}", author_ping, header);
                }
                _ => {
                    return;
                }
            }
    

            relay_msg.content = format!("{}\n{}", reply_header, strip_reply(event.content.body().to_owned()));
        }
        let discord_msg = discord::relay::relay_message(relay_msg.clone()).await;
        chat_service::create_message(relay_msg.message, discord_msg);
        // send our message to the room we found the "!party" command in
        // the last parameter is an optional transaction id which we don't
        // care about.
        /*
        let res = room.send(content, None).await.unwrap();
        // https://github.com/matrix-org/matrix-rust-sdk/blob/ae79fd0af5721e78268a9716cb111d9498b51788/bindings/matrix-sdk-ffi/src/room.rs edit code show in bindings
        let replacement = Replacement::new(
            res.event_id,
            MessageType::text_plain("Too much partying!")
        );
        let mut edited_content = RoomMessageEventContent::text_plain("Too much partying!");
        edited_content.relates_to = Some(Relation::Replacement(replacement));
        room.timeline().await.send(edited_content.into(), None).await;

        //room.redact(&res.event_id, Some("Deletion"), None).await;
        room.redact(&event.event_id, Some("Deletion"), None).await;
        println!("message sent");*/
    }
}

pub async fn start_bot() -> Result<()> {
    // Currently this causes a stack overflow on windows, stack size has been increased during compilation as a temporary fix.
    // TODO: Find better fix

    //env::set_var("RUST_LOG", "matrix_sdk=debug,matrix_sdk_appservice=debug");
    tracing_subscriber::fmt::init();

    println!("Starting!");

    let homeserver_url: String = CONFIG.homeserver_url.clone();
    let server_name: String = CONFIG.server_name.clone();

    let registration_local = Some(AppServiceRegistration::try_from_yaml_file(
        "./appservice-registration.yaml",
    )?);

    println!("Loaded config!");

    let appservice_local = Some(
        AppServiceBuilder::new(
            homeserver_url.parse()?,
            server_name.parse()?,
            registration_local.clone().unwrap().clone(),
        )
        .build()
        .await?,
    );

    println!("Created appservice!");

    appservice_local
        .as_ref()
        .unwrap()
        .register_user_query(Box::new(|_, _| Box::pin(async { true })))
        .await;

    println!("Run query");

    let main_bot_name = format!(
        "{}{}",
        registration_local
            .as_ref()
            .unwrap()
            .sender_localpart
            .clone(),
        "bot"
    );
    let res = appservice_local
        .as_ref()
        .unwrap()
        .register_user(&main_bot_name, None)
        .await;
    if res.is_err() {
        println!("Failed to register! This either means account already exists or appservice isn't setup correctly!");
    }
    println!("Created user!");

    let user = appservice_local
        .as_ref()
        .unwrap()
        .user(Some(&main_bot_name))
        .await?;
    let changed_name = user
        .account()
        .set_display_name(Some("Discord Relay"))
        .await
        .is_ok();
    if !changed_name {
        println!("Failed to set display name");
    }

    for mroom in CONFIG.room.iter() {
        let roomid = mroom.matrix.clone();
        let id: Box<RoomId> = RoomId::parse_box(roomid.as_ref()).unwrap();
        user.join_room_by_id(id.as_ref()).await?;
    }

    println!("Joined rooms");

    // This runs the code in a seperate scope, so that it will not keep the mutexes locked.
    {
        *(BOT_REGISTRATION
            .lock()
            .expect("Bot registration is poisoned")) = registration_local.clone();

        *(BOT_APPSERVICE
            .lock()
            .expect("Bot appservice is poisoned")) = appservice_local.clone();

        *(BOT_CLIENT
            .lock()
            .expect("Bot client is poisoned")) = Some(user.clone());
    }

    println!("Syncing");

    // Sync to prevent handling old messages
    let syncres: SyncResponse = user.sync_once(SyncSettings::default()).await.unwrap();

    println!("Registering events");

    user.add_event_handler_context(appservice_local.clone());
    user.add_event_handler(handle_room_message);

    print!("Splitting");

    // Appservice should be accessible by the server!
    //let (host, port) = appservice_local.as_ref().unwrap().registration().get_host_and_port()?;
    // Appservice may not be hosted on same server as matrix server, so we allow it to be set seperately
    let host: Vec<&str> = CONFIG.host.split(":").collect();

    println!("Starting!");

    future::join(
        run_appservice(appservice_local.clone().unwrap(), host),
        sync_bot(user, syncres),
    )
    .await
    .0
    .ok();

    println!("Done!");
    Ok(())
}

pub async fn run_appservice(appservice: AppService, host: Vec<&str>) -> Result<()> {
    appservice
        .run(host[0].to_owned(), host[1].parse::<u16>().unwrap())
        .await?;
    Ok(())
}

pub async fn sync_bot(user: Client, syncres: SyncResponse) -> Result<()> {
    let settings = SyncSettings::default().token(syncres.next_batch);
    user.sync(settings).await.expect("Error during sync!");
    return Ok(());
}
