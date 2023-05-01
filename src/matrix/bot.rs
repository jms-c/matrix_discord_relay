use std::{cell::RefCell, env, string, sync::Mutex};

use futures::future;
use ruma::{
    api::client::appservice,
    api::{appservice::Registration, client::error::ErrorKind},
    events::room::message::{RoomMessageEvent, TextMessageEventContent},
    events::{
        room::{message::{OriginalSyncRoomMessageEvent, RoomMessageEventContent, MessageType, Relation}, member::RoomMemberEventContent},
        AnyMessageLikeEventContent, OriginalSyncMessageLikeEvent, relation::{Replacement, InReplyTo}, StateEventContent,
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
use tracing_subscriber::fmt::format;

use crate::{
    chat_service::{self, Message, User, FullMessage},
    CONFIG,
};

pub static BOT_APPSERVICE: Mutex<Option<AppService>> = Mutex::new(None);
pub static BOT_REGISTRATION: Mutex<Option<AppServiceRegistration>> = Mutex::new(None);

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
        let content = RoomMessageEventContent::text_plain("🎉🎊🥳 let's PARTY!! 🥳🎊🎉");

        println!("sending");

        // send our message to the room we found the "!party" command in
        // the last parameter is an optional transaction id which we don't
        // care about.
        /*let res = room.send(content, None).await.unwrap();
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

    env::set_var("RUST_LOG", "matrix_sdk=debug,matrix_sdk_appservice=debug");
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
            .expect("Bot registration is poisoned")) = appservice_local.clone();
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
