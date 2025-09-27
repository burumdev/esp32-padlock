use core::sync::atomic::{AtomicBool, Ordering};

use embassy_net::tcp::TcpSocket;
use embassy_net::IpListenEndpoint;
use embassy_time::{Duration, Timer};

use esp_mbedtls::{asynch::Session, Certificates, Mode, TlsVersion};
use esp_mbedtls::{Tls, TlsError, X509};

use esp_println::println;

use crate::network::STATIC_IP;
use crate::WEB_TASK_POOL_SIZE;

const PASSWORD_DEVICE: &str = env!["PASSWORD_DEVICE"];
const MOD: &str = "SERVER";

#[embassy_executor::task(pool_size = WEB_TASK_POOL_SIZE)]
pub async fn serve(
    stack: embassy_net::Stack<'static>,
    tls: &'static Tls<'static>,
    device_lock: &'static AtomicBool,
) -> ! {
    let mut tcp_rx_buffer = [0; 4096];
    let mut tcp_tx_buffer = [0; 4096];

    loop {
        if stack.is_link_up() {
            break;
        }
        Timer::after(Duration::from_millis(500)).await;
    }

    println!("{MOD}: Point your browser to https://{STATIC_IP}/");

    let mut socket = TcpSocket::new(stack, &mut tcp_rx_buffer, &mut tcp_tx_buffer);
    socket.set_timeout(Some(Duration::from_secs(10)));

    loop {
        let mut has_error = false;

        println!("{MOD}: Waiting for connection...");
        let r = socket
            .accept(IpListenEndpoint {
                addr: None,
                port: 443,
            })
            .await;
        println!("{MOD}: Connected...");

        if let Err(e) = r {
            println!("{MOD}: Connection error: {:?}", e);
            continue;
        }

        let mut session = Session::new(
            &mut socket,
            Mode::Server,
            TlsVersion::Tls1_2,
            Certificates {
                // Use self-signed certificates
                certificate: X509::pem(concat!(include_str!("../certs/cert.pem"), "\0").as_bytes())
                    .ok(),
                private_key: X509::pem(concat!(include_str!("../certs/key.pem"), "\0").as_bytes())
                    .ok(),
                ..Default::default()
            },
            tls.reference(),
        )
        .unwrap();

        println!("{MOD}: Polling TLS sessions");
        let mut buffer = [0u8; 1024];
        let mut pos = 0;
        let lock_pass_token = "/lock?password=";
        let unlock_pass_token = "/unlock?password=";
        match session.connect().await {
            Ok(()) => {
                println!("{MOD}: Got TLS session");

                let mut req_processed = false;
                loop {
                    match session.read(&mut buffer).await {
                        Ok(0) => {
                            break;
                        }
                        Ok(len) => {
                            let req =
                                unsafe { core::str::from_utf8_unchecked(&buffer[..(pos + len)]) };

                            if !req_processed {
                                if let Some(get_request) =
                                    req.lines().find(|line| line.starts_with("GET"))
                                {
                                    if get_request.contains(lock_pass_token) {
                                        has_error =
                                            toggle_lock(req, device_lock, lock_pass_token, true);
                                    }

                                    if get_request.contains(unlock_pass_token) {
                                        has_error =
                                            toggle_lock(req, device_lock, unlock_pass_token, false);
                                    }

                                    req_processed = true;
                                }
                            }

                            if req.contains("\r\n\r\n") {
                                //print!("{}", req);
                                println!();
                                break;
                            }

                            pos += len;
                        }
                        Err(e) => {
                            println!("{MOD}: read error: {:?}", e);
                            break;
                        }
                    };
                }

                let head = b"HTTP/1.0 200 OK\r\n\r\n";

                let device_locked = device_lock.load(Ordering::Acquire);
                let page = if device_locked {
                    include_str!("locked.html")
                } else {
                    include_str!("unlocked.html")
                };

                if has_error {
                    let repl = page.replace("no-error", "error");
                    session
                        .write(&[head, repl.as_bytes()].concat())
                        .await
                        .unwrap();
                } else {
                    session
                        .write(&[head, page.as_bytes()].concat())
                        .await
                        .unwrap();
                };

                Timer::after(Duration::from_millis(1000)).await;
            }
            Err(TlsError::NoClientCertificate) => {
                println!("{MOD}: Error: No client certificates given. Please provide client certificates during your request");
            }
            Err(TlsError::MbedTlsError(-30592)) => {
                println!("{MOD}: Fatal: Please enable the exception for a self-signed certificate in your browser");
            }
            Err(error) => {
                panic!("{MOD}: {:?}", error);
            }
        }

        drop(session);
        println!("{MOD}: Closing socket");
        socket.close();
        Timer::after(Duration::from_millis(1000)).await;

        socket.abort();
    }
}

fn toggle_lock(req: &str, device_lock: &AtomicBool, pass_token: &str, we_will_lock: bool) -> bool {
    let pass = req
        .split(" ")
        .find(|token| token.contains(pass_token))
        .unwrap()
        .replace(pass_token, "");

    if pass == PASSWORD_DEVICE {
        device_lock.store(we_will_lock, Ordering::Release);

        return false;
    }

    true
}
