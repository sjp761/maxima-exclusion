use std::time::Duration;
use std::{io::ErrorKind, net::TcpListener, sync::Arc};

use anyhow::Result;

use log::info;
use tokio::{sync::Mutex, time::sleep};

use crate::{core::Maxima, lsx::connection::Connection};

pub async fn start_server(port: u16, maxima: Arc<Mutex<Maxima>>) -> Result<()> {
    let addr = "127.0.0.1:".to_string() + port.to_string().as_str();

    let listener = TcpListener::bind(&addr)?;
    listener.set_nonblocking(true)?;
    info!("Listening on: {}", addr);

    let mut connections: Vec<Connection> = Vec::new();

    loop {
        let new_maxima = maxima.clone();
        for conn in &mut connections {
            conn.listen().await.unwrap();
        }

        let (socket, addr) = match listener.accept() {
            Ok(s) => s,
            Err(err) => {
                let kind = err.kind();
                if kind == ErrorKind::WouldBlock {
                    sleep(Duration::from_millis(10)).await;
                    continue;
                }

                panic!("Internal error in LSX server: {}", kind);
            }
        };

        info!("Got a connection from {:?}", addr);
        let mut conn = Connection::new(new_maxima, socket);
        conn.send_challenge().unwrap();
        connections.push(conn);
    }
}
