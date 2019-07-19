// Copyright 2019. The Tari Project
//
// Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
// following conditions are met:
//
// 1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
// disclaimer.
//
// 2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
// following disclaimer in the documentation and/or other materials provided with the distribution.
//
// 3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
// products derived from this software without specific prior written permission.
//
// THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
// INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
// DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
// SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
// SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
// WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
// USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

use chrono::{NaiveDateTime, Utc};
use tari_comms::types::CommsPublicKey;

use crate::{
    schema::{contacts, received_messages, sent_messages, settings},
    text_message_service::error::TextMessageError,
    types::HashDigest,
};

use diesel::{prelude::*, query_dsl::RunQueryDsl, SqliteConnection};
use digest::Digest;
use serde::{Deserialize, Serialize};
use std::{
    cmp::Ordering,
    convert::{TryFrom, TryInto},
};
use tari_comms::{
    connection::NetAddress,
    message::{Message, MessageError},
};
use tari_p2p::tari_message::{ExtendedMessage, TariMessageType};
use tari_utilities::{byte_array::ByteArray, hex::Hex};

/// This function generates a unique ID hash for a Text Message from the message components and an index integer
///
/// `index`: This value should be incremented for every message sent to the same destination. This ensures that if you
/// send a duplicate message to the same destination that the ID hashes will be unique
pub fn generate_id<D: Digest>(
    source_pub_key: &CommsPublicKey,
    dest_pub_key: &CommsPublicKey,
    message: &str,
    timestamp: &NaiveDateTime,
    index: usize,
) -> Vec<u8>
{
    D::new()
        .chain(source_pub_key.as_bytes())
        .chain(dest_pub_key.as_bytes())
        .chain(message.as_bytes())
        .chain(timestamp.to_string())
        .chain(index.to_le_bytes())
        .result()
        .to_vec()
}

/// Represents a single Text Message to be sent that includes an acknowledged field
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct SentTextMessage {
    pub id: Vec<u8>,
    pub source_pub_key: CommsPublicKey,
    pub dest_pub_key: CommsPublicKey,
    pub message: String,
    pub timestamp: NaiveDateTime,
    pub acknowledged: bool,
}

/// The Native Sql version of the SentTextMessage model
#[derive(Insertable, Queryable)]
#[table_name = "sent_messages"]
pub struct SentTextMessageSql {
    pub id: Vec<u8>,
    pub source_pub_key: String,
    pub dest_pub_key: String,
    pub message: String,
    pub timestamp: NaiveDateTime,
    pub acknowledged: i32,
}

impl SentTextMessage {
    pub fn new(source_pub_key: CommsPublicKey, dest_pub_key: CommsPublicKey, message: String) -> SentTextMessage {
        let timestamp = Utc::now().naive_utc();
        // TODO implement automatic calculation of the index of messages sent to this recipient to ensure that rapid
        // duplicate messages have different IDs
        let id = generate_id::<HashDigest>(&source_pub_key, &dest_pub_key, &message, &timestamp, 0);
        SentTextMessage {
            id,
            source_pub_key,
            dest_pub_key,
            message,
            timestamp,
            acknowledged: false,
        }
    }

    pub fn commit(&self, conn: &SqliteConnection) -> Result<(), TextMessageError> {
        diesel::insert_into(sent_messages::table)
            .values(SentTextMessageSql::from(self.clone()))
            .execute(conn)?;
        Ok(())
    }

    pub fn find(id: &Vec<u8>, conn: &SqliteConnection) -> Result<SentTextMessage, TextMessageError> {
        SentTextMessage::try_from(
            sent_messages::table
                .filter(sent_messages::id.eq(id))
                .first::<SentTextMessageSql>(conn)?,
        )
    }

    pub fn find_by_dest_pub_key(
        dest_pub_key: &CommsPublicKey,
        conn: &SqliteConnection,
    ) -> Result<Vec<SentTextMessage>, TextMessageError>
    {
        let mut result = sent_messages::table
            .filter(sent_messages::dest_pub_key.eq(dest_pub_key.to_hex()))
            .order_by(sent_messages::timestamp)
            .load::<SentTextMessageSql>(conn)?;
        let mut deserialized: Vec<Result<SentTextMessage, TextMessageError>> =
            result.drain(..).map(SentTextMessage::try_from).collect();
        // Check if there are any elements that failed to deserialize, if there are fail the whole
        // find_by_dest_pub_key() process
        if deserialized.iter().any(Result::is_err) {
            return Err(TextMessageError::DatabaseDeserializationError);
        }

        Ok(deserialized.drain(..).filter_map(Result::ok).collect())
    }

    pub fn index(conn: &SqliteConnection) -> Result<Vec<SentTextMessage>, TextMessageError> {
        let mut result = sent_messages::table.load::<SentTextMessageSql>(conn)?;

        let mut deserialized: Vec<Result<SentTextMessage, TextMessageError>> =
            result.drain(..).map(SentTextMessage::try_from).collect();
        // Check if there are any elements that failed to deserialize, if there are fail the whole index() process
        if deserialized.iter().any(Result::is_err) {
            return Err(TextMessageError::DatabaseDeserializationError);
        }

        Ok(deserialized.drain(..).filter_map(Result::ok).collect())
    }
}

impl From<SentTextMessage> for SentTextMessageSql {
    fn from(msg: SentTextMessage) -> SentTextMessageSql {
        SentTextMessageSql {
            id: msg.id,
            source_pub_key: msg.source_pub_key.to_hex(),
            dest_pub_key: msg.dest_pub_key.to_hex(),
            message: msg.message,
            timestamp: msg.timestamp,
            acknowledged: msg.acknowledged as i32,
        }
    }
}

impl TryFrom<SentTextMessageSql> for SentTextMessage {
    type Error = TextMessageError;

    fn try_from(msg: SentTextMessageSql) -> Result<Self, Self::Error> {
        Ok(SentTextMessage {
            id: msg.id,
            source_pub_key: CommsPublicKey::from_hex(msg.source_pub_key.as_str())?,
            dest_pub_key: CommsPublicKey::from_hex(msg.dest_pub_key.as_str())?,
            message: msg.message,
            timestamp: msg.timestamp,
            acknowledged: msg.acknowledged != 0,
        })
    }
}

/// Represents a single received Text Message
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct TextMessage {
    pub id: Vec<u8>,
    pub source_pub_key: CommsPublicKey,
    pub dest_pub_key: CommsPublicKey,
    pub message: String,
    pub timestamp: NaiveDateTime,
}

/// The Native Sql version of the TextMessage model
#[derive(Queryable, Insertable)]
#[table_name = "received_messages"]
pub struct TextMessageSql {
    pub id: Vec<u8>,
    pub source_pub_key: String,
    pub dest_pub_key: String,
    pub message: String,
    pub timestamp: NaiveDateTime,
}

impl TextMessage {
    // Does not require new as these will only ever be received
    pub fn commit(&self, conn: &SqliteConnection) -> Result<(), TextMessageError> {
        diesel::insert_into(received_messages::table)
            .values(TextMessageSql::from(self.clone()))
            .execute(conn)?;
        Ok(())
    }

    pub fn index(conn: &SqliteConnection) -> Result<Vec<TextMessage>, TextMessageError> {
        let mut result = received_messages::table.load::<TextMessageSql>(conn)?;
        let mut deserialized: Vec<Result<TextMessage, TextMessageError>> =
            result.drain(..).map(TextMessage::try_from).collect();
        // Check if there are any elements that failed to deserialize, if there are fail the whole index() process
        if deserialized.iter().any(Result::is_err) {
            return Err(TextMessageError::DatabaseDeserializationError);
        }
        Ok(deserialized.drain(..).filter_map(Result::ok).collect())
    }

    pub fn find(id: &Vec<u8>, conn: &SqliteConnection) -> Result<TextMessage, TextMessageError> {
        TextMessage::try_from(
            received_messages::table
                .filter(received_messages::id.eq(id))
                .first::<TextMessageSql>(conn)?,
        )
    }

    pub fn find_by_source_pub_key(
        source_pub_key: &CommsPublicKey,
        conn: &SqliteConnection,
    ) -> Result<Vec<TextMessage>, TextMessageError>
    {
        let mut result = received_messages::table
            .filter(received_messages::source_pub_key.eq(source_pub_key.to_hex()))
            .order_by(received_messages::timestamp)
            .load::<TextMessageSql>(conn)?;
        let mut deserialized: Vec<Result<TextMessage, TextMessageError>> =
            result.drain(..).map(TextMessage::try_from).collect();
        // Check if there are any elements that failed to deserialize, if there are fail the whole
        // find_by_source_pub_key() process
        if deserialized.iter().any(Result::is_err) {
            return Err(TextMessageError::DatabaseDeserializationError);
        }

        Ok(deserialized.drain(..).filter_map(Result::ok).collect())
    }
}

impl From<TextMessage> for TextMessageSql {
    fn from(msg: TextMessage) -> TextMessageSql {
        TextMessageSql {
            id: msg.id,
            source_pub_key: msg.source_pub_key.to_hex(),
            dest_pub_key: msg.dest_pub_key.to_hex(),
            message: msg.message,
            timestamp: msg.timestamp,
        }
    }
}

impl TryFrom<TextMessageSql> for TextMessage {
    type Error = TextMessageError;

    fn try_from(msg: TextMessageSql) -> Result<Self, Self::Error> {
        Ok(TextMessage {
            id: msg.id,
            source_pub_key: CommsPublicKey::from_hex(msg.source_pub_key.as_str())?,
            dest_pub_key: CommsPublicKey::from_hex(msg.dest_pub_key.as_str())?,
            message: msg.message,
            timestamp: msg.timestamp,
        })
    }
}

impl From<TextMessage> for SentTextMessage {
    fn from(t: TextMessage) -> SentTextMessage {
        SentTextMessage {
            id: t.id,
            source_pub_key: t.source_pub_key,
            dest_pub_key: t.dest_pub_key,
            message: t.message,
            timestamp: t.timestamp,
            acknowledged: false,
        }
    }
}

impl From<SentTextMessage> for TextMessage {
    fn from(t: SentTextMessage) -> TextMessage {
        TextMessage {
            id: t.id,
            source_pub_key: t.source_pub_key,
            dest_pub_key: t.dest_pub_key,
            message: t.message,
            timestamp: t.timestamp,
        }
    }
}

impl TryInto<Message> for TextMessage {
    type Error = MessageError;

    fn try_into(self) -> Result<Message, Self::Error> {
        (TariMessageType::new(ExtendedMessage::Text), self).try_into()
    }
}

impl PartialOrd<TextMessage> for TextMessage {
    /// Orders OutboundMessage from least to most time remaining from being scheduled
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        self.timestamp.partial_cmp(&other.timestamp)
    }
}

impl Ord for TextMessage {
    /// Orders OutboundMessage from least to most time remaining from being scheduled
    fn cmp(&self, other: &Self) -> Ordering {
        self.timestamp.cmp(&other.timestamp)
    }
}

/// A message service contact
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
pub struct Contact {
    pub screen_name: String,
    pub pub_key: CommsPublicKey,
    pub address: NetAddress,
}

/// The Native Sql version of the Contact model
#[derive(Queryable, Insertable)]
#[table_name = "contacts"]
pub struct ContactSql {
    pub pub_key: String,
    pub screen_name: String,
    pub address: String,
}

impl Contact {
    pub fn new(screen_name: String, pub_key: CommsPublicKey, address: NetAddress) -> Contact {
        Contact {
            screen_name,
            pub_key,
            address,
        }
    }

    pub fn commit(&self, conn: &SqliteConnection) -> Result<(), TextMessageError> {
        diesel::insert_into(contacts::table)
            .values(ContactSql::from(self.clone()))
            .execute(conn)?;
        Ok(())
    }

    pub fn index(conn: &SqliteConnection) -> Result<Vec<Contact>, TextMessageError> {
        let mut result = contacts::table.load::<ContactSql>(conn)?;

        let mut deserialized: Vec<Result<Contact, TextMessageError>> =
            result.drain(..).map(Contact::try_from).collect();

        // Check if there are any elements that failed to deserialize, if there are fail the whole index() process
        if deserialized.iter().any(Result::is_err) {
            return Err(TextMessageError::DatabaseDeserializationError);
        }

        Ok(deserialized.drain(..).filter_map(Result::ok).collect())
    }

    pub fn find(pub_key: &CommsPublicKey, conn: &SqliteConnection) -> Result<Contact, TextMessageError> {
        Ok(Contact::try_from(
            contacts::table
                .filter(contacts::pub_key.eq(pub_key.to_hex()))
                .first::<ContactSql>(conn)?,
        )?)
    }

    pub fn update(
        &mut self,
        updated_contact: UpdateContact,
        conn: &SqliteConnection,
    ) -> Result<Contact, TextMessageError>
    {
        let updated = diesel::update(contacts::table.filter(contacts::pub_key.eq(&self.pub_key.to_hex())))
            .set(UpdateContactSql::from(updated_contact))
            .execute(conn)?;
        if updated == 0 {
            return Err(TextMessageError::DatabaseUpdateError);
        }

        Ok(Contact::find(&self.pub_key, conn)?)
    }
}

impl From<Contact> for ContactSql {
    fn from(c: Contact) -> ContactSql {
        ContactSql {
            screen_name: c.screen_name,
            pub_key: c.pub_key.to_hex(),
            address: format!("{}", c.address),
        }
    }
}

impl TryFrom<ContactSql> for Contact {
    type Error = TextMessageError;

    fn try_from(c: ContactSql) -> Result<Self, Self::Error> {
        Ok(Contact {
            screen_name: c.screen_name,
            pub_key: CommsPublicKey::from_hex(c.pub_key.as_str())?,
            address: c.address.parse()?,
        })
    }
}

/// The updatable fields of message contact
#[derive(Clone, Debug, PartialEq)]
pub struct UpdateContact {
    pub screen_name: Option<String>,
    pub address: Option<NetAddress>,
}

/// The Native Sql version of the UpdateContact model
#[derive(AsChangeset)]
#[table_name = "contacts"]
pub struct UpdateContactSql {
    pub screen_name: Option<String>,
    pub address: Option<String>,
}

impl From<UpdateContact> for UpdateContactSql {
    fn from(c: UpdateContact) -> UpdateContactSql {
        UpdateContactSql {
            screen_name: c.screen_name,
            address: c.address.map(|a| format!("{}", a)),
        }
    }
}

/// Struct to hold the current settings for the
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TextMessageSettings {
    pub pub_key: CommsPublicKey,
    pub screen_name: String,
}

#[derive(Debug, Queryable, Insertable)]
#[table_name = "settings"]
pub struct TextMessageSettingsSql {
    pub_key: String,
    screen_name: String,
}

impl TextMessageSettings {
    pub fn new(screen_name: String, pub_key: CommsPublicKey) -> TextMessageSettings {
        TextMessageSettings { screen_name, pub_key }
    }

    pub fn commit(&self, conn: &SqliteConnection) -> Result<(), TextMessageError> {
        // There should only be one row in this table (until we support revisions) so first clean out the table
        diesel::delete(settings::table).execute(conn)?;

        // And then insert
        diesel::insert_into(settings::table)
            .values(TextMessageSettingsSql::from(self.clone()))
            .execute(conn)?;
        Ok(())
    }

    pub fn read(conn: &SqliteConnection) -> Result<TextMessageSettings, TextMessageError> {
        let mut result = settings::table.load::<TextMessageSettingsSql>(conn)?;

        let mut deserialized: Vec<Result<TextMessageSettings, TextMessageError>> =
            result.drain(..).map(TextMessageSettings::try_from).collect();

        // Check if there are any elements that failed to deserialize, if there are fail the whole index() process
        if deserialized.iter().any(Result::is_err) {
            return Err(TextMessageError::DatabaseDeserializationError);
        }

        if deserialized.len() != 1 {
            return Err(TextMessageError::SettingsReadError);
        }

        if let Ok(s) = deserialized.remove(0) {
            Ok(s)
        } else {
            Err(TextMessageError::SettingsReadError)
        }
    }
}

impl From<TextMessageSettings> for TextMessageSettingsSql {
    fn from(c: TextMessageSettings) -> TextMessageSettingsSql {
        TextMessageSettingsSql {
            screen_name: c.screen_name,
            pub_key: c.pub_key.to_hex(),
        }
    }
}

impl TryFrom<TextMessageSettingsSql> for TextMessageSettings {
    type Error = TextMessageError;

    fn try_from(c: TextMessageSettingsSql) -> Result<Self, Self::Error> {
        Ok(TextMessageSettings {
            screen_name: c.screen_name,
            pub_key: CommsPublicKey::from_hex(c.pub_key.as_str())?,
        })
    }
}

#[cfg(test)]
mod test {
    use crate::text_message_service::{
        model::{SentTextMessage, TextMessageSettings},
        Contact,
        TextMessage,
        UpdateContact,
    };
    use chrono::Utc;
    use diesel::{Connection, SqliteConnection};
    use std::path::PathBuf;
    use tari_comms::types::CommsPublicKey;
    use tari_crypto::keys::PublicKey;

    fn get_path(name: Option<&str>) -> String {
        let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        path.push("tests/data");
        path.push(name.unwrap_or(""));
        path.to_str().unwrap().to_string()
    }

    fn clean_up(name: &str) {
        if std::fs::metadata(get_path(Some(name))).is_ok() {
            std::fs::remove_file(get_path(Some(name))).unwrap();
        }
    }

    fn init(name: &str) {
        clean_up(name);
        let path = get_path(None);
        let _ = std::fs::create_dir(&path).unwrap_or_default();
    }

    #[test]
    fn db_model_tests() {
        let mut rng = rand::OsRng::new().unwrap();
        let (_secret_key1, public_key1) = CommsPublicKey::random_keypair(&mut rng);
        let (_secret_key2, public_key2) = CommsPublicKey::random_keypair(&mut rng);
        let (_secret_key3, public_key3) = CommsPublicKey::random_keypair(&mut rng);

        let db_name = "test.sqlite3";
        let db_path = get_path(Some(db_name));
        init(db_name);

        embed_migrations!("./migrations");
        let conn = SqliteConnection::establish(&db_path).unwrap_or_else(|_| panic!("Error connecting to {}", db_path));
        conn.execute("PRAGMA foreign_keys = ON").unwrap();

        embedded_migrations::run_with_output(&conn, &mut std::io::stdout()).expect("Migration failed");

        let _settings1 = TextMessageSettings::new("Bob".to_string(), public_key1.clone()).commit(&conn);
        let read_settings1 = TextMessageSettings::read(&conn).unwrap();
        assert_eq!(read_settings1.screen_name, "Bob".to_string());
        let _settings2 = TextMessageSettings::new("Ed".to_string(), public_key1.clone()).commit(&conn);
        let read_settings2 = TextMessageSettings::read(&conn).unwrap();
        assert_eq!(read_settings2.screen_name, "Ed".to_string());

        let mut contact1 = Contact::new(
            "Alice".to_string(),
            public_key2.clone(),
            "127.0.0.1:45532".parse().unwrap(),
        );

        contact1.commit(&conn).unwrap();

        let contact2 = Contact::new(
            "Bob".to_string(),
            public_key3.clone(),
            "127.0.0.1:45532".parse().unwrap(),
        );

        contact2.commit(&conn).unwrap();

        let contacts = Contact::index(&conn).unwrap();

        assert_eq!(contacts, vec![contact1.clone(), contact2.clone()]);

        let update = UpdateContact {
            screen_name: Some("Carol".to_string()),
            address: None,
        };

        let contact1 = contact1.update(update, &conn).unwrap();

        let contacts = Contact::index(&conn).unwrap();

        assert_eq!(contacts, vec![contact1.clone(), contact2.clone()]);
        assert_eq!(contact2, Contact::find(&contact2.pub_key.clone(), &conn).unwrap());

        assert!(
            SentTextMessage::new(public_key1.clone(), public_key1.clone(), "Test1".to_string())
                .commit(&conn)
                .is_err()
        );

        let sent_msg1 = SentTextMessage::new(public_key1.clone(), public_key2.clone(), "Test1".to_string());
        sent_msg1.commit(&conn).unwrap();
        let sent_msg2 = SentTextMessage::new(public_key1.clone(), public_key3.clone(), "Test2".to_string());
        sent_msg2.commit(&conn).unwrap();
        let sent_msg3 = SentTextMessage::new(public_key1.clone(), public_key3.clone(), "Test3".to_string());
        sent_msg3.commit(&conn).unwrap();

        let sent_msgs = SentTextMessage::index(&conn).unwrap();
        assert_eq!(sent_msgs, vec![sent_msg1.clone(), sent_msg2.clone(), sent_msg3.clone()]);
        let find1 = SentTextMessage::find(&sent_msg1.id, &conn).unwrap();
        assert_eq!(find1, sent_msg1);
        let find2 = SentTextMessage::find_by_dest_pub_key(&public_key3.clone(), &conn).unwrap();
        assert_eq!(find2, vec![sent_msg2, sent_msg3]);

        let recv_msg1 = TextMessage {
            id: vec![1u8; 32],
            source_pub_key: public_key1.clone(),
            dest_pub_key: public_key2.clone(),
            message: "recv1".to_string(),
            timestamp: Utc::now().naive_utc(),
        };
        recv_msg1.commit(&conn).unwrap();
        let recv_msg2 = TextMessage {
            id: vec![2u8; 32],
            source_pub_key: public_key2.clone(),
            dest_pub_key: public_key3.clone(),
            message: "recv2".to_string(),
            timestamp: Utc::now().naive_utc(),
        };
        recv_msg2.commit(&conn).unwrap();
        let recv_msg3 = TextMessage {
            id: vec![3u8; 32],
            source_pub_key: public_key2.clone(),
            dest_pub_key: public_key3.clone(),
            message: "recv3".to_string(),
            timestamp: Utc::now().naive_utc(),
        };
        recv_msg3.commit(&conn).unwrap();

        let recv_msgs = TextMessage::index(&conn).unwrap();
        assert_eq!(recv_msgs, vec![recv_msg1.clone(), recv_msg2.clone(), recv_msg3.clone()]);
        let find1 = TextMessage::find(&recv_msg1.id, &conn).unwrap();
        assert_eq!(find1, recv_msg1);
        let find2 = TextMessage::find_by_source_pub_key(&public_key2.clone(), &conn).unwrap();
        assert_eq!(find2, vec![recv_msg2, recv_msg3]);

        clean_up(db_name);
    }
}