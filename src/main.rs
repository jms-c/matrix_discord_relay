#[macro_use]
extern crate lazy_static;
extern crate toml;

use std::{rc::Rc, sync::{Mutex, Arc}};

use matrix::bot::BOT_REGISTRATION;
use rusqlite::{Connection, Result};
use anyhow::Ok;
use futures::{future};
use serde::Deserialize;

pub mod discord;
pub mod matrix;
pub mod chat_service;

#[derive(Debug, Deserialize, Clone)]
pub struct Outer {
    pub discord_token: String,

    pub host: String,
    pub homeserver_url: String,
    pub server_name: String,
    
    pub room: Vec<Entry>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct Entry {
    pub discord: String,
    pub discord_guild: String,
    pub matrix: String,
    pub webhook: String,
}

lazy_static! {
    pub static ref CONFIG: Outer = load_config();
    //pub static ref DATABASE: Arc<Connection> = Arc::new(Connection::open("./relay.db").expect("Error loading db!"));
    pub static ref INIT_TESTS: Mutex<bool> = Mutex::new(false);
}

#[tokio::main]
pub async fn main() -> anyhow::Result<()> {    
    init_statics().await?;
    //return Ok(());
    
    // Both wait on event loop of some kind, so we run them at the same time
        //futures::join!(matrix_bot::start_bot(), discord_bot::start_bot()).await;
    future::join(matrix::bot::start_bot(), discord::bot::start_bot()).await.0.ok();

    Ok(())
}

pub fn load_config() -> Outer
{
    let config_str: String = std::fs::read_to_string("./config.toml").expect("Failed to read config file!");
    let config_parsed: Outer = toml::from_str(&config_str).expect("Failed to deserialize config!");
    return config_parsed;
}

pub async fn init_statics() -> anyhow::Result<()> {

    //let conn = MutexConnection::open("./relay.db");

    let config_str: String = std::fs::read_to_string("./config.toml").ok().unwrap();
    let config_parsed: Outer = toml::from_str(&config_str)?;
    
    let database = Connection::open("./relay.db").expect("Error loading db!");

    // lock().unwrap() doesn't work here, but try_lock() does.
        database.execute("
            CREATE TABLE IF NOT EXISTS messages (
                id  INTEGER PRIMARY KEY,
                service_org TEXT NOT NULL,
                server_id_org   TEXT NOT NULL,
                room_id_org TEXT NOT NULL,
                id_org  TEXT NOT NULL,
                service_out TEXT NOT NULL,
                server_id_out   TEXT NOT NULL,
                room_id_out TEXT NOT NULL,
                id_out  TEXT NOT NULL UNIQUE
            )
        ", ()).expect("Should have created message");
    

    for val in config_parsed.room.iter() {
        println!("{} -> {}", val.discord, val.matrix);
    }
    Ok(())
}




pub async fn init_tests() {
    // Only run tests once, this appears to be the easiest way of achieving this
    if !(*(INIT_TESTS.lock().unwrap())) {
        init_statics().await.unwrap();
        (*(INIT_TESTS.lock().unwrap())) = true;
    }
}

#[cfg(test)]
mod tests {
    use crate::chat_service::Message;

    use super::*;

    #[tokio::test]
    async fn test_init() {
        init_tests().await;
    }

    #[tokio::test]
    async fn test_db_message()
    {
        init_tests().await;

        let fake_msg1: Message = Message {
            service: "a".to_owned(),
            server_id: "a_sid".to_owned(),
            room_id: "a_rid".to_owned(),
            id: "a_id".to_owned()
        };

        let fake_msg2: Message = Message {
            service: "b".to_owned(),
            server_id: "b_sid".to_owned(),
            room_id: "b_rid".to_owned(),
            id: "b_id".to_owned()
        };
        chat_service::create_message(fake_msg1, fake_msg2);
    }

    #[tokio::test]
    async fn test_db_origin()
    {
        init_tests().await;

        let fake_msg1: Message = Message {
            service: "a".to_owned(),
            server_id: "a_sid".to_owned(),
            room_id: "a_rid".to_owned(),
            id: "a_id".to_owned()
        };

        let fake_msg2: Message = Message {
            service: "b".to_owned(),
            server_id: "b_sid".to_owned(),
            room_id: "b_rid".to_owned(),
            id: "b_id".to_owned()
        };
        chat_service::create_message(fake_msg1.clone(), fake_msg2.clone());


        let origin = chat_service::message_origin(fake_msg2.clone());
        if origin.is_none() {
            panic!("The origin should exist!");
        }
        assert_eq!(origin.unwrap().id, "a_id");


        let origin_noexist = chat_service::message_origin(fake_msg1.clone());
        if origin_noexist.is_some() {
            panic!("The origin shouldn't exist");
        }
    }

    #[tokio::test]
    async fn test_db_relay()
    {
        init_tests().await;

        let fake_msg1: Message = Message {
            service: "a".to_owned(),
            server_id: "a_sid".to_owned(),
            room_id: "a_rid".to_owned(),
            id: "a_id".to_owned()
        };

        let fake_msg2: Message = Message {
            service: "b".to_owned(),
            server_id: "b_sid".to_owned(),
            room_id: "b_rid".to_owned(),
            id: "b_id".to_owned()
        };
        chat_service::create_message(fake_msg1.clone(), fake_msg2.clone());


        let relays = chat_service::message_relays(fake_msg1.clone());
        assert_eq!(relays.len(), 1);

        let relays_noexist = chat_service::message_relays(fake_msg2.clone());
        assert_eq!(relays_noexist.len(), 0);
    }
}