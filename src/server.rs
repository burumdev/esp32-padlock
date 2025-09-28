use core::net::{IpAddr, Ipv4Addr, SocketAddr};
use core::sync::atomic::Ordering;

use edge_nal_embassy::{Tcp, TcpBuffers};
use embassy_time::{Duration, Timer};

use esp_mbedtls::asynch::TlsAcceptor;
use esp_mbedtls::{Certificates, TlsVersion};
use esp_mbedtls::{Tls, X509};

use esp_println::println;

use crate::network::STATIC_IP;
use crate::{DEVICE_LOCK, SERVER_SOCKETS};

use edge_nal::TcpBind;

const RX_SIZE: usize = 4096;
const TX_SIZE: usize = 2048;

const PASSWORD_DEVICE: &str = env!["PASSWORD_DEVICE"];
const MOD: &str = "SERVER";

use edge_http::io::server::{Connection, Handler, Server};
use edge_http::io::Error;
use edge_http::Method;
use embedded_io_async::{Read, Write};

type HttpsServer = Server<SERVER_SOCKETS, RX_SIZE, 32>;
struct HttpHandler;

impl Handler for HttpHandler {
    type Error<E>
        = Error<E>
    where
        E: core::fmt::Debug;

    async fn handle<T, const N: usize>(
        &self,
        _task_id: impl core::fmt::Display + Copy,
        connection: &mut Connection<'_, T, N>,
    ) -> Result<(), Self::Error<T::Error>>
    where
        T: Read + Write,
    {
        let mut has_error = false;
        let headers = connection.headers()?;

        if headers.method == Method::Post {
            //Fill buffer with ASCII space to trim later
            let mut buf = [32; 128];
            let (_, body) = connection.split();

            body.read(&mut buf).await?;
            let body_str = core::str::from_utf8(&buf).unwrap_or("");

            has_error = toggle_lock(body_str.trim());
        }

        connection
            .initiate_response(200, Some("OK"), &[("Content-Type", "text/html")])
            .await?;

        let is_locked = DEVICE_LOCK.load(Ordering::SeqCst);
        let html = if is_locked {
            include_str!("locked.html")
        } else {
            include_str!("unlocked.html")
        };

        if has_error {
            let repl = html.replace("no-error", "error");
            connection.write_all(repl.as_bytes()).await?;
        } else {
            connection.write_all(html.as_bytes()).await?;
        };

        Ok(())
    }
}

#[embassy_executor::task]
pub async fn serve(stack: embassy_net::Stack<'static>, tls: &'static Tls<'static>) -> ! {
    loop {
        if stack.is_link_up() {
            break;
        }
        Timer::after(Duration::from_millis(500)).await;
    }

    println!("{MOD}: Point your browser to https://{STATIC_IP}/");

    let mut server = HttpsServer::new();
    let buffers = TcpBuffers::<SERVER_SOCKETS, TX_SIZE, RX_SIZE>::new();
    let tcp = Tcp::new(stack, &buffers);

    let acceptor = tcp
        .bind(SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), 443))
        .await
        .unwrap();

    let certs = Certificates {
        // Use self-signed certificates
        certificate: X509::pem(concat!(include_str!("../certs/cert.pem"), "\0").as_bytes()).ok(),
        private_key: X509::pem(concat!(include_str!("../certs/key.pem"), "\0").as_bytes()).ok(),
        ..Default::default()
    };

    let timeout = 15_000;
    loop {
        let tls_acceptor = TlsAcceptor::new(&acceptor, TlsVersion::Tls1_2, certs, tls.reference());

        server
            .run(
                Some(timeout),
                edge_nal::WithTimeout::new(timeout, tls_acceptor),
                HttpHandler,
            )
            .await
            .unwrap()
    }
}

fn toggle_lock(body_str: &str) -> bool {
    let pword = body_str.replace("p=", "");

    if pword == PASSWORD_DEVICE {
        let is_locked = DEVICE_LOCK.load(Ordering::Acquire);
        DEVICE_LOCK.store(!is_locked, Ordering::Release);

        return false;
    }

    true
}
