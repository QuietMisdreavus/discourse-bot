// whismur (discourse-bot) - a bot to track duration between certain topics
// Copyright (C) 2017 QuietMisdreavus
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program.  If not, see <http://www.gnu.org/licenses/>.

#[macro_use] extern crate serde_derive;
extern crate serde;
extern crate serde_json;
extern crate chrono;
extern crate irc;

use std::collections::HashMap;
use std::fs::File;

use chrono::{DateTime, UTC};

use irc::client::prelude::*;

const FILE_NAME: &'static str = "discourse.json";

#[derive(Debug, Serialize, Deserialize)]
struct Discourse {
    last_mention: DateTime<UTC>,
    record: Option<u64>,
}

impl Discourse {
    fn new() -> Discourse {
        Discourse {
            last_mention: UTC::now(),
            record: None,
        }
    }

    fn days_since_last(&self) -> u64 {
        UTC::now().signed_duration_since(self.last_mention).num_days() as u64
    }

    fn fine_time_since_last(&self) -> (u64, u64, u64) {
        let now = UTC::now();
        (now.signed_duration_since(self.last_mention).num_hours() as u64,
         now.signed_duration_since(self.last_mention).num_minutes() as u64 % 60,
         now.signed_duration_since(self.last_mention).num_seconds() as u64 % 60)
    }

    fn reset(&mut self) {
        let current = Some(self.days_since_last());
        self.last_mention = UTC::now();
        self.record = std::cmp::max(self.record, current);
    }
}

fn main() {
    let irc_conf = Config::load("config.json").unwrap();

    println!("Connecting...");

    let srv = IrcServer::from_config(irc_conf).unwrap();
    srv.identify().unwrap();

    println!("Ready!");

    let my_nick = srv.config().nickname.as_ref().unwrap().as_str();

    let mut disco_tracker = if let Ok(f) = File::open(FILE_NAME) {
        serde_json::from_reader(f).unwrap()
    } else {
        HashMap::new()
    };

    for msg in srv.iter() {
        let msg = msg.unwrap();
        match msg.command {
            Command::JOIN(ref channel, _, _) => {
                if let &Some(ref prefix) = &msg.prefix {
                    if prefix.starts_with(my_nick) {
                        println!("Joined to {}.", channel);
                    }
                }
            }
            Command::PRIVMSG(ref target, ref text) => {
                let text = text.trim();
                if let Some(nick) = msg.source_nickname() {
                    let (target, cmd): (&str, Option<&str>) = if target == my_nick {
                        (nick, Some(text))
                    } else {
                        let cmd = if text.starts_with(my_nick) {
                            let text = &text[my_nick.len()..];
                            if text.starts_with(&[',', ':'][..]) {
                                Some(text[1..].trim())
                            } else {
                                None
                            }
                        } else {
                            None
                        };

                        (target.as_str(), cmd)
                    };

                    if let Some(cmd) = cmd {
                        {
                            let tracker = disco_tracker.entry(target.to_owned())
                                .or_insert_with(HashMap::new);
                            let disco = tracker.entry(cmd.to_lowercase())
                                .or_insert_with(Discourse::new);

                            if let Some(record) = disco.record {
                                let current = disco.days_since_last();
                                if current == 0 {
                                    let (hour, min, sec) = disco.fine_time_since_last();
                                    srv.send_notice(target,
                                                    &format!("It has been {}:{:02}:{:02} since {} \
                                                             discussed \"{}\". Record: [{}] days",
                                                             hour, min, sec, target, cmd,
                                                             record)).unwrap();
                                } else {
                                    srv.send_notice(target,
                                                    &format!("It has been [{}] days since {} \
                                                             discussed \"{}\". Record: [{}]",
                                                             current, target, cmd, record)).unwrap();
                                }
                                disco.reset();
                            } else {
                                disco.record = Some(0);
                                srv.send_notice(target,
                                                &format!("Now tracking \"{}\" for {}.",
                                                         cmd, target)).unwrap();
                            }
                        }

                        match File::create(FILE_NAME) {
                            Ok(f) => {
                                serde_json::to_writer(f, &disco_tracker).unwrap();
                            }
                            Err(e) => println!("ERROR: couldn't open file for writing: {}", e),
                        }
                    }
                }
            }
            _ => {}
        }
    }
}
