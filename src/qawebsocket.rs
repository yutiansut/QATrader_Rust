use websocket::{OwnedMessage, Message, WebSocketError, ClientBuilder};
use websocket::receiver::Reader;
use websocket::sender::Writer;
use std::net::TcpStream;
use serde_json::Value;
use log::{warn, error, debug, info};
use crossbeam_channel::{Sender, Receiver};
use crate::msg::{parse_message, RtnData};
use crate::xmsg::{XPeek, XReqLogin};
use crate::config::CONFIG;
use crate::scheduler::Event;
use std::str::from_utf8;

pub struct QAWebSocket {
    pub sender: Writer<TcpStream>,
    pub receiver: Reader<TcpStream>,
}

impl QAWebSocket {
    pub fn connect(wsuri: &str) -> Result<(Writer<TcpStream>, Reader<TcpStream>), WebSocketError> {
        let client = ClientBuilder::new(wsuri)
            .unwrap()
            .add_protocol("rust-websocket")
            .connect_insecure()?;

        let (receiver, sender) = client.split().unwrap();
        Ok((sender, receiver))
    }

    pub fn login(mut ws_send: Sender<OwnedMessage>) {
        let account = CONFIG.common.account.clone();
        let password = CONFIG.common.password.clone();
        let broker = CONFIG.common.broker.clone();
        let login = XReqLogin {
            topic: "login".to_string(),
            aid: "req_login".to_string(),
            bid: broker.clone(),
            user_name: account.clone(),
            password: password.clone(),
        };
        let msg = serde_json::to_string(&login).unwrap();
        if let Err(e) = ws_send.send(OwnedMessage::Text(msg)) {
            error!("Login Error: {:?}", e);
        }
    }

    /// 从本地chanel接收消息-->往websocket 发送消息
    pub fn send_loop(mut sender: Writer<TcpStream>, rx: Receiver<OwnedMessage>, mut s_c: Sender<Event>) {
        loop {
            // Send loop
            let message = match rx.recv() {
                Ok(m) => m,
                Err(e) => {
                    error!("Receive Channel Error: {:?}", e);
                    continue;
                }
            };
            match message {
                OwnedMessage::Ping(_) => {
                    let _ = sender.send_message(&message);
                }
                OwnedMessage::Text(str) => {
                    match parse_message(str) {
                        Some(data) => {
                            let x = OwnedMessage::Text(data);
                            if let Err(e) = sender.send_message(&x) {
                                error!("Send WebSocket {:?}", e);
                                break;
                            }
                        }
                        None => {
                            error!("Send Cancel,消息格式错误/未知消息");
                        }
                    }
                }
                _ => {
                    error!("内部错误")
                }
            };
        }
        info!("send_loop exit");
    }

    /// 接收websokcet 消息
    pub fn receive_loop(mut receiver: Reader<TcpStream>, mut ws_send: Sender<OwnedMessage>, mut db_send: Sender<String>, mut s_c: Sender<Event>) {
        let mut Error_count = 1;
        for message in receiver.incoming_messages() {
            {
                // Peek
                let peek = r#"{"topic":"peek","aid":"peek_message"}"#.to_string();
                ws_send.send(OwnedMessage::Text(peek));
            }

            match message {
                Ok(om) => {
                    match om {
                        OwnedMessage::Close(_) => {
                            break;
                        }
                        OwnedMessage::Text(msg) => {
                            info!("Receive WebSocket Data: {:?}", msg);
                            db_send.send(msg);
                        }
                        OwnedMessage::Pong(msg) => {
                            let _ = from_utf8(&msg).unwrap().to_string();
                        }
                        _ => ()
                    }
                }
                Err(e) => {
                    error!("Receive WebSocket Error {:?}", Error_count);
                    if Error_count >= 10 {
                        s_c.send(Event::RESTART);
                        break;
                    }
                    Error_count += 1;
                }
            };
        }
        info!("receive_loop exit");
    }
}


