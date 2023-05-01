use rusqlite::Connection;

#[derive(Clone)]
pub struct User {
    pub source: String, // Source, e.g matrix, discord
    pub id: String, // Actual id
    pub ping: String, // Used to mention user
    pub tag: String, // Used to tag (kinda)
    pub display: String // Display Name
}

#[derive(Clone)]
pub struct Message {
    pub service: String,
    pub server_id: String, // Server id, if applicable (not applicable to matrix as it can only work as 1 appservice atm)
    pub room_id: String,
    pub id: String,
}

#[derive(Clone)]
pub struct FullMessage {
    pub user: User,
    pub message: Message,

    pub content: String,
    pub reply: Option<Box<Message>>
}

pub fn create_message(source: Message, relayed: Message)
{
    let database = Connection::open("./relay.db").expect("Error loading db!");
    database.execute("
    INSERT OR IGNORE INTO messages (service_org, server_id_org, room_id_org, id_org, service_out, server_id_out, room_id_out, id_out)
    VALUES (?, ?, ?, ?, ?, ?, ?, ?);",
    (source.service, source.server_id, source.room_id, source.id, relayed.service, relayed.server_id, relayed.room_id, relayed.id)).expect("Failed to insert message into database!");
}

pub fn message_origin(relayed: Message) -> Option<Message>
{
    let database = Connection::open("./relay.db").expect("Error loading db!");
    let mut stmt = database.prepare("SELECT service_org, server_id_org, room_id_org, id_org FROM messages WHERE service_out=:s AND server_id_out=:sid AND room_id_out=:rid AND id_out=:id").unwrap();
    let iter = stmt.query_map(&[
        (":s", relayed.service.as_str()),
        (":sid", relayed.server_id.as_str()),
        (":rid", relayed.room_id.as_str()),
        (":id", relayed.id.as_str()),
    ], |row| {
        Ok(Message {
            service: row.get(0)?,
            server_id: row.get(1)?,
            room_id: row.get(2)?,
            id: row.get(3)?,
        })
    }).unwrap();

    let mut out: Vec<Message> = Vec::new();
    for msg in iter {
        out.push(msg.unwrap());
    }

    if out.len() == 0 {
        return None;
    }

    assert_eq!(out.len(), 1);

    return Some(out[0].clone());
}

pub fn message_relays(source: Message) -> Vec<Message>
{
    let database = Connection::open("./relay.db").expect("Error loading db!");
    let mut stmt = database.prepare("SELECT service_out, server_id_out, room_id_out, id_out FROM messages WHERE service_org=:s AND server_id_org=:sid AND room_id_org=:rid AND id_org=:id").unwrap();
    let iter = stmt.query_map(&[
        (":s", source.service.as_str()),
        (":sid", source.server_id.as_str()),
        (":rid", source.room_id.as_str()),
        (":id", source.id.as_str()),
    ], |row| {
        Ok(Message {
            service: row.get(0)?,
            server_id: row.get(1)?,
            room_id: row.get(2)?,
            id: row.get(3)?,
        })
    }).unwrap();

    let mut out: Vec<Message> = Vec::new();
    for msg in iter {
        out.push(msg.unwrap());
    }

    return out;
}